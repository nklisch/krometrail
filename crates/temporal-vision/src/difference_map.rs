use std::num::NonZeroUsize;

use crate::{
    BinaryMask, ErrorCode, MeasurementParameters, NormalizedSequence, PixelDimensions, Result,
    Rgb8, Timestamp, VisionError,
    measure::{classify_pixel_change, intersecting_gap_count},
};

const DEFAULT_MAX_BYTES: usize = 256 * 1024 * 1024;
const ACCUMULATOR_BYTES_PER_PIXEL: usize = 48;

stable_registry! {
    /// Quantity encoded as brightness in the change-frequency panel.
    pub enum FrequencyMode {
        Count => "count",
        Magnitude => "magnitude",
        NormalizedFrequency => "normalized_frequency",
    }
}

stable_registry! {
    /// Deterministic palette used by the change-timing panel.
    pub enum TimePalette {
        Spectral => "spectral",
    }
}

/// Working-memory and output bounds for one difference-map request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DifferenceMapLimits {
    max_accumulator_bytes: NonZeroUsize,
    max_output_bytes: NonZeroUsize,
}

impl DifferenceMapLimits {
    pub const fn new(max_accumulator_bytes: NonZeroUsize, max_output_bytes: NonZeroUsize) -> Self {
        Self {
            max_accumulator_bytes,
            max_output_bytes,
        }
    }

    pub const fn max_accumulator_bytes(self) -> usize {
        self.max_accumulator_bytes.get()
    }

    pub const fn max_output_bytes(self) -> usize {
        self.max_output_bytes.get()
    }
}

impl Default for DifferenceMapLimits {
    fn default() -> Self {
        Self::new(
            NonZeroUsize::new(DEFAULT_MAX_BYTES).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).expect("default is nonzero"),
        )
    }
}

/// Deterministic choices for one temporal difference map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DifferenceMapParameters {
    reference_frame_index: usize,
    frequency_mode: FrequencyMode,
    time_palette: TimePalette,
    repeated_change_separation: Option<Timestamp>,
    measurement: MeasurementParameters,
    background: Rgb8,
    limits: DifferenceMapLimits,
}

impl DifferenceMapParameters {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        reference_frame_index: usize,
        frequency_mode: FrequencyMode,
        time_palette: TimePalette,
        repeated_change_separation: Option<Timestamp>,
        measurement: MeasurementParameters,
        background: Rgb8,
        limits: DifferenceMapLimits,
    ) -> Self {
        Self {
            reference_frame_index,
            frequency_mode,
            time_palette,
            repeated_change_separation,
            measurement,
            background,
            limits,
        }
    }

    pub const fn reference_frame_index(self) -> usize {
        self.reference_frame_index
    }
    pub const fn frequency_mode(self) -> FrequencyMode {
        self.frequency_mode
    }
    pub const fn time_palette(self) -> TimePalette {
        self.time_palette
    }
    pub const fn repeated_change_separation(self) -> Option<Timestamp> {
        self.repeated_change_separation
    }
    pub const fn measurement(self) -> MeasurementParameters {
        self.measurement
    }
    pub const fn background(self) -> Rgb8 {
        self.background
    }
    pub const fn limits(self) -> DifferenceMapLimits {
        self.limits
    }
}

#[derive(Debug)]
pub(crate) struct DifferenceAccumulators {
    dimensions: PixelDimensions,
    analysis_mask: Option<BinaryMask>,
    change_count: Box<[u32]>,
    comparable_count: Box<[u32]>,
    magnitude_sum: Box<[u64]>,
    weighted_time_sum: Box<[u128]>,
    first_change_offset: Box<[u64]>,
    last_change_offset: Box<[u64]>,
}

impl DifferenceAccumulators {
    pub(crate) fn accumulate<F>(
        normalized: &NormalizedSequence<F>,
        measurement: MeasurementParameters,
        limits: DifferenceMapLimits,
    ) -> Result<Self> {
        let pixel_count = normalized.dimensions().pixel_count()?;
        let accumulator_bytes = pixel_count
            .checked_mul(ACCUMULATOR_BYTES_PER_PIXEL)
            .ok_or_else(accumulator_limit_error)?;
        if accumulator_bytes > limits.max_accumulator_bytes() {
            return Err(accumulator_limit_error());
        }

        let mut accumulators = Self {
            dimensions: normalized.dimensions(),
            analysis_mask: normalized.analysis_mask().cloned(),
            change_count: vec![0; pixel_count].into_boxed_slice(),
            comparable_count: vec![0; pixel_count].into_boxed_slice(),
            magnitude_sum: vec![0; pixel_count].into_boxed_slice(),
            weighted_time_sum: vec![0; pixel_count].into_boxed_slice(),
            first_change_offset: vec![0; pixel_count].into_boxed_slice(),
            last_change_offset: vec![0; pixel_count].into_boxed_slice(),
        };
        let range_start = normalized.frames()[0].timestamp().as_nanos();
        let width = usize::try_from(normalized.dimensions().width())
            .map_err(|_| accumulator_limit_error())?;

        for frames in normalized.frames().windows(2) {
            let earlier = &frames[0];
            let later = &frames[1];
            if intersecting_gap_count(
                normalized.gap_ranges(),
                earlier.timestamp(),
                later.timestamp(),
            ) > 0
            {
                continue;
            }
            let later_offset = later
                .timestamp()
                .as_nanos()
                .checked_sub(range_start)
                .ok_or_else(accumulator_limit_error)?;
            for (pixel, (before, after)) in earlier
                .linear_rgb16()
                .chunks_exact(3)
                .zip(later.linear_rgb16().chunks_exact(3))
                .enumerate()
            {
                let x = u32::try_from(pixel % width).map_err(|_| accumulator_limit_error())?;
                let y = u32::try_from(pixel / width).map_err(|_| accumulator_limit_error())?;
                if accumulators
                    .analysis_mask
                    .as_ref()
                    .is_some_and(|mask| mask.includes(x, y) != Some(true))
                {
                    continue;
                }
                accumulators.comparable_count[pixel] = accumulators.comparable_count[pixel]
                    .checked_add(1)
                    .ok_or_else(accumulator_limit_error)?;
                let before: &[u16; 3] = before
                    .try_into()
                    .expect("chunks_exact yields three-channel pixels");
                let after: &[u16; 3] = after
                    .try_into()
                    .expect("chunks_exact yields three-channel pixels");
                let change = classify_pixel_change(before, after, measurement)?;
                if !change.changed {
                    continue;
                }
                let magnitude =
                    u64::try_from(change.weighted_square).map_err(|_| accumulator_limit_error())?;
                let count = accumulators.change_count[pixel]
                    .checked_add(1)
                    .ok_or_else(accumulator_limit_error)?;
                accumulators.change_count[pixel] = count;
                accumulators.magnitude_sum[pixel] = accumulators.magnitude_sum[pixel]
                    .checked_add(magnitude)
                    .ok_or_else(accumulator_limit_error)?;
                let weighted_time = u128::from(later_offset)
                    .checked_mul(change.weighted_square)
                    .ok_or_else(accumulator_limit_error)?;
                accumulators.weighted_time_sum[pixel] = accumulators.weighted_time_sum[pixel]
                    .checked_add(weighted_time)
                    .ok_or_else(accumulator_limit_error)?;
                if count == 1 {
                    accumulators.first_change_offset[pixel] = later_offset;
                }
                accumulators.last_change_offset[pixel] = later_offset;
            }
        }
        Ok(accumulators)
    }
}

pub(crate) struct DifferenceMapData {
    accumulators: DifferenceAccumulators,
    range_start: Timestamp,
    range_duration_ns: u64,
    effective_separation_ns: u64,
    frequency_mode: FrequencyMode,
    max_change_count: u32,
    max_magnitude: u64,
}

impl DifferenceMapData {
    pub(crate) fn build<F>(
        normalized: &NormalizedSequence<F>,
        parameters: DifferenceMapParameters,
    ) -> Result<Self> {
        let accumulators = DifferenceAccumulators::accumulate(
            normalized,
            parameters.measurement,
            parameters.limits,
        )?;
        let range_start = normalized.frames()[0].timestamp();
        let range_duration_ns = normalized
            .frames()
            .last()
            .expect("normalized sequence is nonempty")
            .timestamp()
            .as_nanos()
            .checked_sub(range_start.as_nanos())
            .ok_or_else(accumulator_limit_error)?;
        let effective_separation_ns = parameters
            .repeated_change_separation
            .map(Timestamp::as_nanos)
            .unwrap_or_else(|| (range_duration_ns / 4).max(1));
        let max_change_count = accumulators.change_count.iter().copied().max().unwrap_or(0);
        let max_magnitude = accumulators
            .magnitude_sum
            .iter()
            .copied()
            .max()
            .unwrap_or(0);
        Ok(Self {
            accumulators,
            range_start,
            range_duration_ns,
            effective_separation_ns,
            frequency_mode: parameters.frequency_mode,
            max_change_count,
            max_magnitude,
        })
    }

    pub(crate) fn dimensions(&self) -> PixelDimensions {
        self.accumulators.dimensions
    }

    pub(crate) fn frequency_value(&self, pixel: usize) -> Option<u32> {
        let comparable = *self.accumulators.comparable_count.get(pixel)?;
        if comparable == 0 {
            return None;
        }
        let value = match self.frequency_mode {
            FrequencyMode::Count => scale_to_byte(
                u64::from(self.accumulators.change_count[pixel]),
                u64::from(self.max_change_count),
            ),
            FrequencyMode::Magnitude => {
                scale_to_byte(self.accumulators.magnitude_sum[pixel], self.max_magnitude)
            }
            FrequencyMode::NormalizedFrequency => scale_to_byte(
                u64::from(self.accumulators.change_count[pixel]),
                u64::from(comparable),
            ),
        };
        Some(value)
    }

    pub(crate) fn is_repeated_change(&self, pixel: usize) -> bool {
        self.accumulators
            .change_count
            .get(pixel)
            .is_some_and(|count| {
                *count >= 2
                    && self.accumulators.last_change_offset[pixel]
                        - self.accumulators.first_change_offset[pixel]
                        >= self.effective_separation_ns
            })
    }

    pub(crate) fn timing_offset(&self, pixel: usize) -> Option<u64> {
        let magnitude = *self.accumulators.magnitude_sum.get(pixel)?;
        if magnitude == 0 {
            return None;
        }
        u64::try_from(self.accumulators.weighted_time_sum[pixel] / u128::from(magnitude)).ok()
    }

    pub(crate) const fn range_start(&self) -> Timestamp {
        self.range_start
    }
    pub(crate) const fn range_duration_ns(&self) -> u64 {
        self.range_duration_ns
    }
    pub(crate) const fn effective_separation_ns(&self) -> u64 {
        self.effective_separation_ns
    }
    pub(crate) const fn max_change_count(&self) -> u32 {
        self.max_change_count
    }
    pub(crate) const fn max_magnitude(&self) -> u64 {
        self.max_magnitude
    }
}

fn scale_to_byte(value: u64, maximum: u64) -> u32 {
    if maximum == 0 {
        return 0;
    }
    u32::try_from((u128::from(value) * 255) / u128::from(maximum))
        .expect("a normalized byte is at most 255")
}

fn accumulator_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "difference-map accumulation exceeds configured integer or memory limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DeclaredGap, Frame, FrameSequence, IntegerScale, Marker, NormalizationParameters,
        PixelFormat, ProcessingLimits, TimeRange, normalize_sequence,
    };

    fn normalized(
        frames: Vec<(u8, u64, [u8; 4])>,
        gaps: Vec<DeclaredGap<u8>>,
        mask: Option<BinaryMask>,
    ) -> NormalizedSequence<u8> {
        let dimensions = PixelDimensions::new(1, 1).unwrap();
        let source = FrameSequence::new(
            frames
                .into_iter()
                .map(|(id, time, pixels)| {
                    Frame::new(
                        id,
                        Timestamp::from_nanos(time),
                        dimensions,
                        PixelFormat::Rgba8SrgbStraight,
                        pixels.to_vec().into_boxed_slice(),
                    )
                    .unwrap()
                })
                .collect(),
            Vec::<Marker<u8>>::new(),
            gaps,
            None,
            mask,
        )
        .unwrap();
        normalize_sequence(
            &source,
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
    fn accumulation_is_exact_gap_aware_repeated_and_bounded() {
        let sequence = normalized(
            vec![
                (1, 0, [0, 0, 0, 255]),
                (2, 10, [255, 255, 255, 255]),
                (3, 30, [0, 0, 0, 255]),
            ],
            Vec::new(),
            None,
        );
        let parameters = DifferenceMapParameters::new(
            0,
            FrequencyMode::Count,
            TimePalette::Spectral,
            Some(Timestamp::from_nanos(20)),
            MeasurementParameters::new(0),
            Rgb8::new(1, 2, 3),
            DifferenceMapLimits::default(),
        );
        let data = DifferenceMapData::build(&sequence, parameters).unwrap();
        let magnitude = 65_536_u64 * 65_535_u64 * 65_535_u64;
        assert_eq!(data.accumulators.change_count[0], 2);
        assert_eq!(data.accumulators.comparable_count[0], 2);
        assert_eq!(data.accumulators.magnitude_sum[0], magnitude * 2);
        assert_eq!(data.timing_offset(0), Some(20));
        assert!(data.is_repeated_change(0));
        assert_eq!(data.frequency_value(0), Some(255));

        let gap = DeclaredGap::new(
            1,
            TimeRange::new(Timestamp::from_nanos(20), Timestamp::from_nanos(20)).unwrap(),
            "loss",
            None,
        )
        .unwrap();
        let with_gap = normalized(
            vec![
                (1, 0, [0, 0, 0, 255]),
                (2, 10, [255, 255, 255, 255]),
                (3, 30, [0, 0, 0, 255]),
            ],
            vec![gap],
            None,
        );
        let data = DifferenceMapData::build(&with_gap, parameters).unwrap();
        assert_eq!(data.accumulators.change_count[0], 1);
        assert_eq!(data.accumulators.comparable_count[0], 1);
        assert!(!data.is_repeated_change(0));

        let tiny = DifferenceMapLimits::new(
            NonZeroUsize::new(47).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        );
        assert_eq!(
            DifferenceAccumulators::accumulate(&sequence, MeasurementParameters::new(0), tiny)
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}
