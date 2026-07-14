use std::{collections::BTreeMap, num::NonZeroUsize};

use crate::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, BinaryMask, EncodedImage, ErrorCode,
    EvidenceClass, FrameSequence, GeneratedArtifact, MeasurementParameters, NormalizationKind,
    NormalizationStep, NormalizedSequence, ParameterValue, Parameters, PixelDimensions, PixelRect,
    Result, Rgb8, Timestamp, VisionError,
    measure::{classify_pixel_change, intersecting_gap_count, linear_luminance},
    render::{
        canvas::{BLACK, Canvas, MUTED, PANEL, WARNING, WHITE, canvas_limit_error},
        font::{CELL_WIDTH, draw_text, ellipsize},
    },
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

const ALGORITHM_NAME: &str = "temporal-difference-map";
const ALGORITHM_VERSION: &str = "v1";
const MARGIN: u32 = 16;
const INTER_PANEL_GAP: u32 = 16;
const HEADER_HEIGHT: u32 = 56;
const PANEL_LABEL_HEIGHT: u32 = 28;
const LEGEND_HEIGHT: u32 = 120;
const SECTION_GAP: u32 = 12;
const REPEATED_COLOR: [u8; 3] = [255, 78, 142];
const UNAVAILABLE_COLOR: [u8; 3] = [62, 68, 79];
const PALETTE_STOPS: [(u8, [u8; 3]); 3] = [
    (0, [40, 104, 224]),
    (128, [190, 66, 174]),
    (255, [250, 190, 48]),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DifferenceMapLayout {
    image: PixelDimensions,
    header: PixelRect,
    reference_panel: PixelRect,
    frequency_panel: PixelRect,
    timing_panel: PixelRect,
    reference_label: PixelRect,
    frequency_label: PixelRect,
    timing_label: PixelRect,
    legend: PixelRect,
}

impl DifferenceMapLayout {
    pub(crate) fn new(panel: PixelDimensions) -> Result<Self> {
        let image_width = MARGIN
            .checked_mul(2)
            .and_then(|value| value.checked_add(panel.width().checked_mul(3)?))
            .and_then(|value| value.checked_add(INTER_PANEL_GAP.checked_mul(2)?))
            .ok_or_else(canvas_limit_error)?;
        let image_height = MARGIN
            .checked_mul(2)
            .and_then(|value| value.checked_add(HEADER_HEIGHT))
            .and_then(|value| value.checked_add(SECTION_GAP))
            .and_then(|value| value.checked_add(PANEL_LABEL_HEIGHT))
            .and_then(|value| value.checked_add(panel.height()))
            .and_then(|value| value.checked_add(SECTION_GAP))
            .and_then(|value| value.checked_add(LEGEND_HEIGHT))
            .ok_or_else(canvas_limit_error)?;
        let image =
            PixelDimensions::new(image_width, image_height).map_err(|_| canvas_limit_error())?;
        let label_y = MARGIN + HEADER_HEIGHT + SECTION_GAP;
        let panel_y = label_y + PANEL_LABEL_HEIGHT;
        let reference_x = MARGIN;
        let frequency_x = reference_x
            .checked_add(panel.width())
            .and_then(|value| value.checked_add(INTER_PANEL_GAP))
            .ok_or_else(canvas_limit_error)?;
        let timing_x = frequency_x
            .checked_add(panel.width())
            .and_then(|value| value.checked_add(INTER_PANEL_GAP))
            .ok_or_else(canvas_limit_error)?;
        let legend_y = panel_y
            .checked_add(panel.height())
            .and_then(|value| value.checked_add(SECTION_GAP))
            .ok_or_else(canvas_limit_error)?;
        Ok(Self {
            image,
            header: PixelRect::new(MARGIN, MARGIN, image_width - 2 * MARGIN, HEADER_HEIGHT)?,
            reference_panel: PixelRect::new(reference_x, panel_y, panel.width(), panel.height())?,
            frequency_panel: PixelRect::new(frequency_x, panel_y, panel.width(), panel.height())?,
            timing_panel: PixelRect::new(timing_x, panel_y, panel.width(), panel.height())?,
            reference_label: PixelRect::new(
                reference_x,
                label_y,
                panel.width(),
                PANEL_LABEL_HEIGHT,
            )?,
            frequency_label: PixelRect::new(
                frequency_x,
                label_y,
                panel.width(),
                PANEL_LABEL_HEIGHT,
            )?,
            timing_label: PixelRect::new(timing_x, label_y, panel.width(), PANEL_LABEL_HEIGHT)?,
            legend: PixelRect::new(MARGIN, legend_y, image_width - 2 * MARGIN, LEGEND_HEIGHT)?,
        })
    }
}

/// Difference-map artifact using the crate-wide encoded-image and manifest seam.
pub type DifferenceMapArtifact<ArtifactId, FrameId, MarkerId, GapId> =
    GeneratedArtifact<ArtifactId, FrameId, MarkerId, GapId>;

/// Render one deterministic, source-derived temporal difference map.
pub fn render_difference_map<A, F, M, G, P>(
    artifact_id: A,
    sequence: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: DifferenceMapParameters,
) -> Result<DifferenceMapArtifact<A, F, M, G>>
where
    F: Clone + Eq,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    validate_normalized_source(sequence, normalized, parameters.reference_frame_index)?;
    let data = DifferenceMapData::build(normalized, parameters)?;
    let layout = DifferenceMapLayout::new(data.dimensions())?;
    let canvas_bytes = layout
        .image
        .pixel_count()?
        .checked_mul(3)
        .ok_or_else(canvas_limit_error)?;
    if canvas_bytes > parameters.limits.max_output_bytes() {
        return Err(canvas_limit_error());
    }

    let mut canvas = Canvas::new(
        layout.image,
        parameters.background.channels(),
        parameters.limits.max_output_bytes(),
    )?;
    draw_composite(&mut canvas, layout, sequence, normalized, parameters, &data)?;
    let (bytes, hash) = crate::encode::encode_png(
        layout.image,
        canvas.pixels(),
        parameters.limits.max_output_bytes(),
    )?;

    let mut normalization = normalized.normalization_steps().to_vec();
    normalization.push(parameters.measurement.provenance_step()?);
    normalization.push(display_step()?);
    let manifest = ArtifactManifest::from_sequence(
        artifact_id,
        ArtifactKind::DifferenceMap,
        EvidenceClass::SourceDerived,
        AlgorithmDescriptor::new(ALGORITHM_NAME, ALGORITHM_VERSION)?,
        sequence,
        vec![
            sequence.frames()[parameters.reference_frame_index]
                .id()
                .clone(),
        ],
        normalization,
        manifest_parameters(parameters, &data)?,
        layout.image,
        hash,
    )?;
    Ok(GeneratedArtifact::new(
        EncodedImage::new(layout.image, bytes),
        manifest,
    ))
}

fn validate_normalized_source<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    reference_frame_index: usize,
) -> Result<()> {
    if reference_frame_index >= normalized.frames().len()
        || source.frames().len() != normalized.frames().len()
        || source
            .frames()
            .iter()
            .zip(normalized.frames())
            .any(|(source, normalized)| {
                source.id() != normalized.id() || source.timestamp() != normalized.timestamp()
            })
    {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "normalized frames do not match the source sequence or reference index",
        ));
    }
    Ok(())
}

fn draw_composite<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    layout: DifferenceMapLayout,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: DifferenceMapParameters,
    data: &DifferenceMapData,
) -> Result<()> {
    canvas.fill_rect(
        layout.header.x(),
        layout.header.y(),
        layout.header.width(),
        layout.header.height(),
        BLACK,
    )?;
    draw_clipped_text(
        canvas,
        layout.header.x(),
        layout.header.y() + 4,
        layout.header.width(),
        "TEMPORAL DIFFERENCE MAP",
        WHITE,
    )?;
    let range = format!(
        "RANGE {} - {} | TIME ->",
        format_time(source.range().start()),
        format_time(source.range().end())
    );
    draw_clipped_text(
        canvas,
        layout.header.x(),
        layout.header.y() + 18,
        layout.header.width(),
        &range,
        MUTED,
    )?;
    draw_clipped_text(
        canvas,
        layout.reference_label.x(),
        layout.reference_label.y() + 8,
        layout.reference_label.width(),
        "REFERENCE",
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        layout.frequency_label.x(),
        layout.frequency_label.y() + 8,
        layout.frequency_label.width(),
        "CHANGE FREQUENCY",
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        layout.timing_label.x(),
        layout.timing_label.y() + 8,
        layout.timing_label.width(),
        "CHANGE TIMING",
        WHITE,
    )?;

    draw_reference_panel(
        canvas,
        layout.reference_panel,
        &normalized.frames()[parameters.reference_frame_index],
    )?;
    draw_frequency_panel(canvas, layout.frequency_panel, data)?;
    draw_timing_panel(
        canvas,
        layout.timing_panel,
        data,
        parameters.background.channels(),
    )?;
    draw_legend(canvas, layout.legend, source, parameters, data)
}

fn draw_reference_panel<F>(
    canvas: &mut Canvas,
    panel: PixelRect,
    frame: &crate::NormalizedFrame<F>,
) -> Result<()> {
    for (index, pixel) in frame.linear_rgb16().chunks_exact(3).enumerate() {
        let luminance = linear_luminance(pixel)?;
        let byte =
            u8::try_from((luminance * 255 + 32_767) / 65_535).map_err(|_| canvas_limit_error())?;
        set_panel_pixel(canvas, panel, index, [byte; 3])?;
    }
    Ok(())
}

fn draw_frequency_panel(
    canvas: &mut Canvas,
    panel: PixelRect,
    data: &DifferenceMapData,
) -> Result<()> {
    for pixel in 0..data.dimensions().pixel_count()? {
        let color = data
            .frequency_value(pixel)
            .map_or(UNAVAILABLE_COLOR, |value| [value as u8; 3]);
        set_panel_pixel(canvas, panel, pixel, color)?;
    }
    Ok(())
}

fn draw_timing_panel(
    canvas: &mut Canvas,
    panel: PixelRect,
    data: &DifferenceMapData,
    background: [u8; 3],
) -> Result<()> {
    for pixel in 0..data.dimensions().pixel_count()? {
        let color = if data.is_repeated_change(pixel) {
            REPEATED_COLOR
        } else if let Some(offset) = data.timing_offset(pixel) {
            palette_color(offset, data.range_duration_ns())
        } else if data.accumulators.comparable_count[pixel] == 0 {
            UNAVAILABLE_COLOR
        } else {
            background
        };
        set_panel_pixel(canvas, panel, pixel, color)?;
    }
    Ok(())
}

fn set_panel_pixel(
    canvas: &mut Canvas,
    panel: PixelRect,
    index: usize,
    color: [u8; 3],
) -> Result<()> {
    let width = usize::try_from(panel.width()).map_err(|_| canvas_limit_error())?;
    let x = u32::try_from(index % width).map_err(|_| canvas_limit_error())?;
    let y = u32::try_from(index / width).map_err(|_| canvas_limit_error())?;
    canvas.set_pixel(panel.x() + x, panel.y() + y, color)
}

fn draw_legend<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    legend: PixelRect,
    source: &FrameSequence<F, M, G, P>,
    parameters: DifferenceMapParameters,
    data: &DifferenceMapData,
) -> Result<()> {
    canvas.fill_rect(
        legend.x(),
        legend.y(),
        legend.width(),
        legend.height(),
        PANEL,
    )?;
    let frequency = match parameters.frequency_mode {
        FrequencyMode::Count => format!("FREQUENCY: COUNT | MAX {}", data.max_change_count()),
        FrequencyMode::Magnitude => format!("FREQUENCY: MAGNITUDE | MAX {}", data.max_magnitude()),
        FrequencyMode::NormalizedFrequency => "FREQUENCY: NORMALIZED | 0 - 100 PERCENT".to_owned(),
    };
    draw_clipped_text(
        canvas,
        legend.x() + 4,
        legend.y() + 6,
        legend.width().saturating_sub(8),
        &frequency,
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        legend.x() + 4,
        legend.y() + 20,
        legend.width().saturating_sub(8),
        "NO CHANGE -> BRIGHTER MEANS MORE CHANGE",
        MUTED,
    )?;

    let swatch_width = legend.width().saturating_sub(8).clamp(1, 256);
    let swatch = PixelRect::new(legend.x() + 4, legend.y() + 36, swatch_width, 8)?;
    for x in 0..swatch.width() {
        let position = if swatch.width() == 1 {
            0
        } else {
            u64::from(x) * 255 / u64::from(swatch.width() - 1)
        };
        for y in 0..swatch.height() {
            canvas.set_pixel(swatch.x() + x, swatch.y() + y, palette_color(position, 255))?;
        }
    }
    let timing = format!(
        "TIMING: EARLY {} | MID {} | LATE {}",
        format_time(data.range_start()),
        format_time(Timestamp::from_nanos(
            data.range_start().as_nanos() + data.range_duration_ns() / 2
        )),
        format_time(Timestamp::from_nanos(
            data.range_start().as_nanos() + data.range_duration_ns()
        ))
    );
    draw_clipped_text(
        canvas,
        legend.x() + 4,
        legend.y() + 50,
        legend.width().saturating_sub(8),
        &timing,
        MUTED,
    )?;
    canvas.fill_rect(legend.x() + 4, legend.y() + 66, 12, 8, REPEATED_COLOR)?;
    let repeated = format!(
        "REPEATED CHANGE | SEPARATION >= {} NS",
        data.effective_separation_ns()
    );
    draw_clipped_text(
        canvas,
        legend.x() + 20,
        legend.y() + 65,
        legend.width().saturating_sub(24),
        &repeated,
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        legend.x() + 4,
        legend.y() + 82,
        legend.width().saturating_sub(8),
        "SOURCE-DERIVED CHANGE; NO CAUSE OR DIRECTION INFERRED",
        MUTED,
    )?;
    if !source.gaps().is_empty() {
        canvas.fill_rect(legend.x(), legend.y() + 98, legend.width(), 18, WARNING)?;
        draw_clipped_text(
            canvas,
            legend.x() + 4,
            legend.y() + 102,
            legend.width().saturating_sub(8),
            "GAP - UNSEEN BEHAVIOR MAY HAVE OCCURRED",
            BLACK,
        )?;
    }
    Ok(())
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
    draw_text(canvas, x, y, &ellipsize(text, cells), color)
}

fn palette_color(offset: u64, duration: u64) -> [u8; 3] {
    let position = if duration == 0 {
        0
    } else {
        u8::try_from((u128::from(offset.min(duration)) * 255) / u128::from(duration))
            .expect("normalized palette position is at most 255")
    };
    let (start_position, start, end_position, end) = if position <= 128 {
        (
            PALETTE_STOPS[0].0,
            PALETTE_STOPS[0].1,
            PALETTE_STOPS[1].0,
            PALETTE_STOPS[1].1,
        )
    } else {
        (
            PALETTE_STOPS[1].0,
            PALETTE_STOPS[1].1,
            PALETTE_STOPS[2].0,
            PALETTE_STOPS[2].1,
        )
    };
    let span = u32::from(end_position - start_position);
    let position = u32::from(position - start_position);
    std::array::from_fn(|channel| {
        let before = u32::from(start[channel]);
        let after = u32::from(end[channel]);
        let value = if after >= before {
            before + (after - before) * position / span
        } else {
            before - (before - after) * position / span
        };
        value as u8
    })
}

fn format_time(timestamp: Timestamp) -> String {
    let milliseconds = timestamp.as_nanos() / 1_000_000;
    let micros = timestamp.as_nanos() % 1_000_000 / 1_000;
    format!("{milliseconds}.{micros:03} MS")
}

fn display_step() -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::ColorSpaceConversion,
        "difference-map-display-rgb8-v1",
        parameters([
            (
                "reference",
                ParameterValue::Text("linear16_luminance_to_rgb8".into()),
            ),
            ("frequency", ParameterValue::Text("grayscale_rgb8".into())),
            (
                "timing",
                ParameterValue::Text("spectral_integer_palette".into()),
            ),
        ])?,
    )
}

fn manifest_parameters(
    request: DifferenceMapParameters,
    data: &DifferenceMapData,
) -> Result<Parameters> {
    parameters([
        (
            "algorithm_version",
            ParameterValue::Text(ALGORITHM_VERSION.into()),
        ),
        (
            "frequency_mode",
            ParameterValue::Text(request.frequency_mode.as_str().into()),
        ),
        (
            "time_palette",
            ParameterValue::Text(request.time_palette.as_str().into()),
        ),
        (
            "reference_frame_index",
            unsigned_usize(request.reference_frame_index)?,
        ),
        (
            "effective_repeated_separation_nanos",
            ParameterValue::Unsigned(data.effective_separation_ns()),
        ),
        (
            "max_change_count",
            ParameterValue::Unsigned(u64::from(data.max_change_count())),
        ),
        (
            "max_magnitude",
            ParameterValue::Unsigned(data.max_magnitude()),
        ),
        (
            "background_rgb8",
            ParameterValue::List(
                request
                    .background
                    .channels()
                    .into_iter()
                    .map(|channel| ParameterValue::Unsigned(u64::from(channel)))
                    .collect(),
            ),
        ),
        (
            "palette_stops",
            ParameterValue::List(
                PALETTE_STOPS
                    .into_iter()
                    .map(|(position, rgb)| {
                        ParameterValue::Object(
                            [
                                (
                                    "position".into(),
                                    ParameterValue::Unsigned(u64::from(position)),
                                ),
                                (
                                    "rgb8".into(),
                                    ParameterValue::List(
                                        rgb.into_iter()
                                            .map(|channel| {
                                                ParameterValue::Unsigned(u64::from(channel))
                                            })
                                            .collect(),
                                    ),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "layout",
            ParameterValue::Text("fixed_three_panel_v1".into()),
        ),
        ("margin", ParameterValue::Unsigned(u64::from(MARGIN))),
        (
            "inter_panel_gap",
            ParameterValue::Unsigned(u64::from(INTER_PANEL_GAP)),
        ),
        (
            "header_height",
            ParameterValue::Unsigned(u64::from(HEADER_HEIGHT)),
        ),
        (
            "panel_label_height",
            ParameterValue::Unsigned(u64::from(PANEL_LABEL_HEIGHT)),
        ),
        (
            "legend_height",
            ParameterValue::Unsigned(u64::from(LEGEND_HEIGHT)),
        ),
        (
            "section_gap",
            ParameterValue::Unsigned(u64::from(SECTION_GAP)),
        ),
        (
            "encoding",
            ParameterValue::Text("png-0.17.16-rgb8-best-no_filter-no_chunks".into()),
        ),
        (
            "max_accumulator_bytes",
            unsigned_usize(request.limits.max_accumulator_bytes())?,
        ),
        (
            "max_output_bytes",
            unsigned_usize(request.limits.max_output_bytes())?,
        ),
    ])
}

fn parameters<const N: usize>(entries: [(&'static str, ParameterValue); N]) -> Result<Parameters> {
    Parameters::new(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn unsigned_usize(value: usize) -> Result<ParameterValue> {
    Ok(ParameterValue::Unsigned(
        u64::try_from(value).map_err(|_| accumulator_limit_error())?,
    ))
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

    #[test]
    fn frequency_modes_scale_count_magnitude_and_comparable_pairs() {
        fn data(frequency_mode: FrequencyMode) -> DifferenceMapData {
            DifferenceMapData {
                accumulators: DifferenceAccumulators {
                    dimensions: PixelDimensions::new(3, 1).unwrap(),
                    analysis_mask: None,
                    change_count: vec![2, 1, 0].into_boxed_slice(),
                    comparable_count: vec![2, 4, 0].into_boxed_slice(),
                    magnitude_sum: vec![100, 50, 0].into_boxed_slice(),
                    weighted_time_sum: vec![1_000, 500, 0].into_boxed_slice(),
                    first_change_offset: vec![1, 1, 0].into_boxed_slice(),
                    last_change_offset: vec![9, 1, 0].into_boxed_slice(),
                },
                range_start: Timestamp::ZERO,
                range_duration_ns: 10,
                effective_separation_ns: 5,
                frequency_mode,
                max_change_count: 2,
                max_magnitude: 100,
            }
        }

        assert_eq!(
            [0, 1, 2].map(|pixel| data(FrequencyMode::Count).frequency_value(pixel)),
            [Some(255), Some(127), None]
        );
        assert_eq!(
            [0, 1, 2].map(|pixel| data(FrequencyMode::Magnitude).frequency_value(pixel)),
            [Some(255), Some(127), None]
        );
        assert_eq!(
            [0, 1, 2].map(|pixel| data(FrequencyMode::NormalizedFrequency).frequency_value(pixel)),
            [Some(255), Some(63), None]
        );
    }
}
