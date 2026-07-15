use std::{
    collections::BTreeMap,
    mem::size_of,
    num::{NonZeroU8, NonZeroUsize},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    BinaryMask, ErrorCode, Frame, FrameSequence, NormalizationKind, NormalizationStep,
    ParameterValue, Parameters, PixelDimensions, PixelRect, Result, TimeRange, Timestamp,
    VisionError,
};

const MAX_SCALE_FACTOR: u8 = 8;
const DEFAULT_MAX_FRAMES: usize = 4_096;
const DEFAULT_MAX_PIXELS_PER_FRAME: usize = 16_777_216;
const DEFAULT_MAX_RETAINED_BYTES: usize = 512 * 1024 * 1024;

/// An sRGB background used to make straight-alpha input pixels opaque.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Rgb8 {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb8 {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn channels(self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ScaleDirection {
    Identity,
    Up,
    Down,
}

/// A bounded whole-number image scale.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IntegerScale {
    direction: ScaleDirection,
    factor: NonZeroU8,
}

impl IntegerScale {
    pub const IDENTITY: Self = Self {
        direction: ScaleDirection::Identity,
        factor: NonZeroU8::MIN,
    };

    pub fn up(factor: NonZeroU8) -> Result<Self> {
        Self::new(ScaleDirection::Up, factor)
    }

    pub fn down(factor: NonZeroU8) -> Result<Self> {
        Self::new(ScaleDirection::Down, factor)
    }

    fn new(direction: ScaleDirection, factor: NonZeroU8) -> Result<Self> {
        if factor.get() > MAX_SCALE_FACTOR {
            return Err(VisionError::new(
                ErrorCode::InvalidScale,
                "integer scale factor must be between one and eight",
            ));
        }
        if factor.get() == 1 {
            return Ok(Self::IDENTITY);
        }
        Ok(Self { direction, factor })
    }

    pub const fn factor(self) -> u8 {
        self.factor.get()
    }

    pub const fn is_identity(self) -> bool {
        matches!(self.direction, ScaleDirection::Identity)
    }

    pub(crate) fn direction_name(self) -> &'static str {
        match self.direction {
            ScaleDirection::Identity => "identity",
            ScaleDirection::Up => "up",
            ScaleDirection::Down => "down",
        }
    }
}

/// Explicit bounds for one normalization result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessingLimits {
    max_frames: NonZeroUsize,
    max_pixels_per_frame: NonZeroUsize,
    max_retained_bytes: NonZeroUsize,
}

impl ProcessingLimits {
    pub const fn new(
        max_frames: NonZeroUsize,
        max_pixels_per_frame: NonZeroUsize,
        max_retained_bytes: NonZeroUsize,
    ) -> Self {
        Self {
            max_frames,
            max_pixels_per_frame,
            max_retained_bytes,
        }
    }

    pub const fn max_frames(self) -> usize {
        self.max_frames.get()
    }

    pub const fn max_pixels_per_frame(self) -> usize {
        self.max_pixels_per_frame.get()
    }

    pub const fn max_retained_bytes(self) -> usize {
        self.max_retained_bytes.get()
    }
}

impl Default for ProcessingLimits {
    fn default() -> Self {
        Self::new(
            NonZeroUsize::new(DEFAULT_MAX_FRAMES).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_PIXELS_PER_FRAME).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_RETAINED_BYTES).expect("default is nonzero"),
        )
    }
}

/// Fixed normalization choices for one common-geometry sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizationParameters {
    background: Rgb8,
    crop: Option<PixelRect>,
    scale: IntegerScale,
    limits: ProcessingLimits,
}

impl NormalizationParameters {
    pub const fn new(
        background: Rgb8,
        crop: Option<PixelRect>,
        scale: IntegerScale,
        limits: ProcessingLimits,
    ) -> Self {
        Self {
            background,
            crop,
            scale,
            limits,
        }
    }

    pub const fn background(self) -> Rgb8 {
        self.background
    }

    pub const fn crop(self) -> Option<PixelRect> {
        self.crop
    }

    pub const fn scale(self) -> IntegerScale {
        self.scale
    }

    pub const fn limits(self) -> ProcessingLimits {
        self.limits
    }
}

/// One source frame represented as tightly packed opaque linear-light RGB16.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedFrame<FrameId> {
    id: FrameId,
    timestamp: Timestamp,
    dimensions: PixelDimensions,
    linear_rgb16: Arc<[u16]>,
}

pub type SharedNormalizedFrame<FrameId> = NormalizedFrame<FrameId>;

impl<F> NormalizedFrame<F> {
    pub fn new(
        id: F,
        timestamp: Timestamp,
        dimensions: PixelDimensions,
        linear_rgb16: Arc<[u16]>,
    ) -> Result<Self> {
        let expected = dimensions
            .pixel_count()?
            .checked_mul(3)
            .ok_or_else(resource_limit_error)?;
        if linear_rgb16.len() != expected {
            return Err(VisionError::new(
                ErrorCode::PixelLengthMismatch,
                "normalized pixel payload length does not match frame dimensions",
            ));
        }
        Ok(Self {
            id,
            timestamp,
            dimensions,
            linear_rgb16,
        })
    }

    pub fn id(&self) -> &F {
        &self.id
    }

    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub fn linear_rgb16(&self) -> &[u16] {
        &self.linear_rgb16
    }

    pub fn pixels(&self) -> &Arc<[u16]> {
        &self.linear_rgb16
    }
}

/// An immutable normalized geometry epoch and its transformed analysis domain.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedSequence<FrameId> {
    source_dimensions: PixelDimensions,
    source_crop: PixelRect,
    dimensions: PixelDimensions,
    frames: Box<[NormalizedFrame<FrameId>]>,
    analysis_mask: Option<BinaryMask>,
    analysis_pixel_count: u64,
    gap_ranges: Box<[TimeRange]>,
    normalization_steps: Box<[NormalizationStep]>,
}

pub type SharedNormalizedSequence<FrameId> = NormalizedSequence<FrameId>;

impl<F> NormalizedSequence<F> {
    pub fn frames(&self) -> &[NormalizedFrame<F>] {
        &self.frames
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub const fn source_dimensions(&self) -> PixelDimensions {
        self.source_dimensions
    }

    pub const fn source_crop(&self) -> PixelRect {
        self.source_crop
    }

    pub fn analysis_mask(&self) -> Option<&BinaryMask> {
        self.analysis_mask.as_ref()
    }

    pub fn normalization_steps(&self) -> &[NormalizationStep] {
        &self.normalization_steps
    }

    pub fn gap_ranges(&self) -> &[TimeRange] {
        &self.gap_ranges
    }

    pub const fn analysis_pixel_count(&self) -> u64 {
        self.analysis_pixel_count
    }

    /// Assemble a normalized epoch from immutable per-frame buffers without copying pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        source_dimensions: PixelDimensions,
        source_crop: PixelRect,
        dimensions: PixelDimensions,
        frames: Vec<NormalizedFrame<F>>,
        analysis_mask: Option<BinaryMask>,
        analysis_pixel_count: u64,
        gap_ranges: Vec<TimeRange>,
        normalization_steps: Vec<NormalizationStep>,
    ) -> Result<Self>
    where
        F: Eq,
    {
        if !source_crop.fits_within(source_dimensions) {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "normalization crop lies outside the source-frame dimensions",
            ));
        }
        let Some(first) = frames.first() else {
            return Err(VisionError::new(
                ErrorCode::EmptySequence,
                "normalized sequence must not be empty",
            ));
        };
        let expected_frame_bytes = dimensions
            .pixel_count()?
            .checked_mul(3)
            .ok_or_else(resource_limit_error)?;
        let range = TimeRange::new(first.timestamp(), frames.last().unwrap().timestamp())?;
        for (index, frame) in frames.iter().enumerate() {
            if frames[..index].iter().any(|prior| prior.id() == frame.id()) {
                return Err(VisionError::at(
                    ErrorCode::DuplicateIdentifier,
                    "normalized frame identifiers must be unique",
                    index,
                ));
            }
            if index > 0 && frames[index - 1].timestamp() > frame.timestamp() {
                return Err(VisionError::at(
                    ErrorCode::OutOfOrder,
                    "normalized frame timestamps must be nondecreasing",
                    index,
                ));
            }
            if frame.dimensions() != dimensions
                || frame.linear_rgb16().len() != expected_frame_bytes
            {
                return Err(VisionError::at(
                    ErrorCode::IncompatibleFrame,
                    "normalized frames must use common dimensions and pixel payloads",
                    index,
                ));
            }
        }
        let expected_analysis_pixels = match &analysis_mask {
            Some(mask) if mask.dimensions() == dimensions => mask
                .bits()
                .iter()
                .map(|byte| u64::from(byte.count_ones()))
                .sum(),
            Some(_) => {
                return Err(VisionError::new(
                    ErrorCode::InvalidMask,
                    "normalized analysis mask dimensions do not match its frames",
                ));
            }
            None => u64::try_from(dimensions.pixel_count()?).map_err(|_| resource_limit_error())?,
        };
        if expected_analysis_pixels != analysis_pixel_count || analysis_pixel_count == 0 {
            return Err(VisionError::new(
                ErrorCode::EmptyAnalysisDomain,
                "normalized analysis pixel count does not match its mask",
            ));
        }
        validate_time_ranges(&gap_ranges, range)?;
        Ok(Self {
            source_dimensions,
            source_crop,
            dimensions,
            frames: frames.into_boxed_slice(),
            analysis_mask,
            analysis_pixel_count,
            gap_ranges: gap_ranges.into_boxed_slice(),
            normalization_steps: normalization_steps.into_boxed_slice(),
        })
    }
}

fn validate_time_ranges(ranges: &[TimeRange], sequence_range: TimeRange) -> Result<()> {
    for (index, range) in ranges.iter().enumerate() {
        if index > 0 {
            let prior = ranges[index - 1];
            if prior.start() > range.start() || prior.end() > range.start() {
                return Err(VisionError::at(
                    ErrorCode::OutOfOrder,
                    "normalized gap ranges must be ordered and non-overlapping",
                    index,
                ));
            }
        }
        if !sequence_range.contains(range.start()) || !sequence_range.contains(range.end()) {
            return Err(VisionError::at(
                ErrorCode::AnnotationOutOfRange,
                "normalized gap range lies outside the frame range",
                index,
            ));
        }
    }
    Ok(())
}

/// Normalize one validated source frame using the same recipe as a full sequence.
///
/// The service uses this narrow operation for request-lifetime sharing. Sequence-level
/// validation and provenance still happen in [`assemble_normalized_sequence`].
pub fn normalize_frame<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    parameters: NormalizationParameters,
) -> Result<NormalizedFrame<F>> {
    let source_dimensions = frame.dimensions();
    let crop = match parameters.crop {
        Some(crop) if crop.fits_within(source_dimensions) => crop,
        Some(_) => {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "normalization crop lies outside the source-frame dimensions",
            ));
        }
        None => PixelRect::new(0, 0, source_dimensions.width(), source_dimensions.height())?,
    };
    let dimensions = scaled_dimensions(crop, parameters.scale)?;
    let output_pixels = dimensions.pixel_count()?;
    validate_resource_limits(1, output_pixels, false, parameters.limits)?;
    let background = parameters.background.channels().map(linear_channel);
    let allow_opaque_full_frame_fast_path = parameters.crop.is_none();
    normalize_frame_inner(
        frame,
        crop,
        dimensions,
        parameters.scale,
        background,
        allow_opaque_full_frame_fast_path,
    )
}

/// Assemble per-frame normalized results while recomputing all sequence-level invariants.
///
/// In particular, this validates source identity and order before accepting an immutable
/// intermediate from another request. It also rebuilds masks, gaps, and normalization steps so
/// sharing pixel buffers cannot change artifact provenance.
pub fn assemble_normalized_sequence<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
    frames: Vec<NormalizedFrame<F>>,
    parameters: NormalizationParameters,
) -> Result<NormalizedSequence<F>> {
    let source_dimensions = sequence.dimensions();
    let crop = match parameters.crop {
        Some(crop) if crop.fits_within(source_dimensions) => crop,
        Some(_) => {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "normalization crop lies outside the source-frame dimensions",
            ));
        }
        None => PixelRect::new(0, 0, source_dimensions.width(), source_dimensions.height())?,
    };
    let dimensions = scaled_dimensions(crop, parameters.scale)?;
    let output_pixels = dimensions.pixel_count()?;
    let restricted_domain = sequence.region().is_some() || sequence.mask().is_some();
    validate_resource_limits(
        sequence.frames().len(),
        output_pixels,
        restricted_domain,
        parameters.limits,
    )?;
    if frames.len() != sequence.frames().len() {
        return Err(VisionError::new(
            ErrorCode::IncompatibleFrame,
            "normalized frame count does not match the source sequence",
        ));
    }
    for (index, (source, normalized)) in sequence.frames().iter().zip(&frames).enumerate() {
        if source.id() != normalized.id() || source.timestamp() != normalized.timestamp() {
            return Err(VisionError::at(
                ErrorCode::IncompatibleFrame,
                "normalized frame identity does not match its source frame",
                index,
            ));
        }
    }

    let (analysis_mask, analysis_pixel_count) = transformed_analysis_mask(
        sequence,
        crop,
        dimensions,
        parameters.scale,
        restricted_domain,
    )?;
    let gap_ranges = sequence
        .gaps()
        .iter()
        .map(|gap| gap.range())
        .collect::<Vec<_>>();
    let normalization_steps = normalization_steps(parameters, crop, dimensions)?;
    NormalizedSequence::from_parts(
        source_dimensions,
        crop,
        dimensions,
        frames,
        analysis_mask,
        analysis_pixel_count,
        gap_ranges,
        normalization_steps,
    )
}

/// Normalize one validated source geometry epoch.
pub fn normalize_sequence<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
    parameters: NormalizationParameters,
) -> Result<NormalizedSequence<F>> {
    let frames = sequence
        .frames()
        .iter()
        .map(|frame| normalize_frame(frame, parameters))
        .collect::<Result<Vec<_>>>()?;
    assemble_normalized_sequence(sequence, frames, parameters)
}

fn scaled_dimensions(crop: PixelRect, scale: IntegerScale) -> Result<PixelDimensions> {
    let factor = u32::from(scale.factor());
    let (width, height) = match scale.direction {
        ScaleDirection::Identity => (crop.width(), crop.height()),
        ScaleDirection::Up => (
            crop.width().checked_mul(factor).ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidScale,
                    "scaled width exceeds the coordinate space",
                )
            })?,
            crop.height().checked_mul(factor).ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidScale,
                    "scaled height exceeds the coordinate space",
                )
            })?,
        ),
        ScaleDirection::Down => {
            if crop.width() % factor != 0 || crop.height() % factor != 0 {
                return Err(VisionError::new(
                    ErrorCode::InvalidScale,
                    "downscale factor must exactly divide both cropped dimensions",
                ));
            }
            (crop.width() / factor, crop.height() / factor)
        }
    };
    PixelDimensions::new(width, height).map_err(|_| {
        VisionError::new(
            ErrorCode::InvalidScale,
            "scaled dimensions exceed the supported address space",
        )
    })
}

fn validate_resource_limits(
    frame_count: usize,
    output_pixels: usize,
    restricted_domain: bool,
    limits: ProcessingLimits,
) -> Result<()> {
    if frame_count > limits.max_frames() || output_pixels > limits.max_pixels_per_frame() {
        return Err(resource_limit_error());
    }
    let values_per_frame = output_pixels
        .checked_mul(3)
        .ok_or_else(resource_limit_error)?;
    let bytes_per_frame = values_per_frame
        .checked_mul(size_of::<u16>())
        .ok_or_else(resource_limit_error)?;
    let frame_bytes = bytes_per_frame
        .checked_mul(frame_count)
        .ok_or_else(resource_limit_error)?;
    let mask_bytes = if restricted_domain {
        output_pixels
            .checked_add(7)
            .ok_or_else(resource_limit_error)?
            / 8
    } else {
        0
    };
    let retained_bytes = frame_bytes
        .checked_add(mask_bytes)
        .ok_or_else(resource_limit_error)?;
    if retained_bytes > limits.max_retained_bytes() {
        return Err(resource_limit_error());
    }
    Ok(())
}

fn resource_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "normalization result exceeds configured processing limits",
    )
}

fn transformed_analysis_mask<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
    crop: PixelRect,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    restricted_domain: bool,
) -> Result<(Option<BinaryMask>, u64)> {
    let output_pixels = dimensions.pixel_count()?;
    if !restricted_domain {
        return Ok((
            None,
            u64::try_from(output_pixels).map_err(|_| resource_limit_error())?,
        ));
    }

    let mut bits = vec![
        0_u8;
        output_pixels
            .checked_add(7)
            .ok_or_else(resource_limit_error)?
            / 8
    ];
    let mut included = 0_u64;
    for y in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            if output_pixel_in_domain(sequence, crop, scale, x, y) {
                let index = usize::try_from(y)
                    .ok()
                    .and_then(|row| row.checked_mul(usize::try_from(dimensions.width()).ok()?))
                    .and_then(|row| row.checked_add(usize::try_from(x).ok()?))
                    .ok_or_else(resource_limit_error)?;
                bits[index / 8] |= 0x80 >> (index % 8);
                included = included.checked_add(1).ok_or_else(resource_limit_error)?;
            }
        }
    }
    if included == 0 {
        return Err(VisionError::new(
            ErrorCode::EmptyAnalysisDomain,
            "normalization crop, region, and mask have no measurable output pixels",
        ));
    }
    Ok((Some(BinaryMask::new(dimensions, bits)?), included))
}

fn output_pixel_in_domain<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
    crop: PixelRect,
    scale: IntegerScale,
    output_x: u32,
    output_y: u32,
) -> bool {
    let factor = u32::from(scale.factor());
    match scale.direction {
        ScaleDirection::Identity => {
            source_pixel_in_domain(sequence, crop.x() + output_x, crop.y() + output_y)
        }
        ScaleDirection::Up => source_pixel_in_domain(
            sequence,
            crop.x() + output_x / factor,
            crop.y() + output_y / factor,
        ),
        ScaleDirection::Down => {
            let source_x = crop.x() + output_x * factor;
            let source_y = crop.y() + output_y * factor;
            (0..factor).all(|dy| {
                (0..factor).all(|dx| source_pixel_in_domain(sequence, source_x + dx, source_y + dy))
            })
        }
    }
}

fn source_pixel_in_domain<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
    x: u32,
    y: u32,
) -> bool {
    let in_region = sequence.region().is_none_or(|region| {
        let rect = region.rect();
        x >= rect.x()
            && y >= rect.y()
            && x < rect.right_exclusive().expect("validated region")
            && y < rect.bottom_exclusive().expect("validated region")
    });
    in_region
        && sequence
            .mask()
            .is_none_or(|mask| mask.includes(x, y) == Some(true))
}

fn normalize_frame_inner<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    crop: PixelRect,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    background: [u16; 3],
    allow_opaque_full_frame_fast_path: bool,
) -> Result<NormalizedFrame<F>> {
    let capacity = dimensions
        .pixel_count()?
        .checked_mul(3)
        .ok_or_else(resource_limit_error)?;
    if can_use_opaque_full_frame_fast_path(
        frame,
        crop,
        dimensions,
        scale,
        allow_opaque_full_frame_fast_path,
    ) {
        return normalize_opaque_full_frame(frame, dimensions, scale, capacity);
    }
    normalize_frame_general(frame, crop, dimensions, scale, background, capacity)
}

fn can_use_opaque_full_frame_fast_path<F, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    crop: PixelRect,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    allow_opaque_full_frame_fast_path: bool,
) -> bool {
    if !allow_opaque_full_frame_fast_path
        || crop.x() != 0
        || crop.y() != 0
        || crop.width() != frame.dimensions().width()
        || crop.height() != frame.dimensions().height()
    {
        return false;
    }
    let expected_dimensions = match scale.direction {
        ScaleDirection::Identity => frame.dimensions(),
        ScaleDirection::Down => match scaled_dimensions(crop, scale) {
            Ok(dimensions) => dimensions,
            Err(_) => return false,
        },
        ScaleDirection::Up => return false,
    };
    dimensions == expected_dimensions
        && frame
            .pixels()
            .chunks_exact(4)
            .all(|rgba| rgba[3] == u8::MAX)
}

fn normalize_opaque_full_frame<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    capacity: usize,
) -> Result<NormalizedFrame<F>> {
    match scale.direction {
        ScaleDirection::Identity => normalize_opaque_identity(frame, dimensions, capacity),
        ScaleDirection::Down => normalize_opaque_downscale(frame, dimensions, scale, capacity),
        ScaleDirection::Up => unreachable!("opaque fast path excludes upscaling"),
    }
}

fn normalize_opaque_identity<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    dimensions: PixelDimensions,
    capacity: usize,
) -> Result<NormalizedFrame<F>> {
    // The common screenshot shape is already row-major RGBA8. Once opacity and
    // identity geometry are established, coordinate reconstruction and alpha
    // arithmetic cannot contribute any result, so convert the packed channels
    // directly while preserving the existing transfer table and channel order.
    let mut output = Vec::with_capacity(capacity);
    for rgba in frame.pixels().chunks_exact(4) {
        output.push(linear_channel(rgba[0]));
        output.push(linear_channel(rgba[1]));
        output.push(linear_channel(rgba[2]));
    }
    Ok(NormalizedFrame {
        id: frame.id().clone(),
        timestamp: frame.timestamp(),
        dimensions,
        linear_rgb16: Arc::<[u16]>::from(output.into_boxed_slice()),
    })
}

fn normalize_opaque_downscale<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    capacity: usize,
) -> Result<NormalizedFrame<F>> {
    // Keep the existing non-overlapping box average and round-half-up rules,
    // but walk source rows directly instead of reconstructing four generic
    // composited pixels for every opaque output pixel.
    let factor = usize::from(scale.factor());
    let source_width =
        usize::try_from(frame.dimensions().width()).map_err(|_| resource_limit_error())?;
    let output_width = usize::try_from(dimensions.width()).map_err(|_| resource_limit_error())?;
    let output_height = usize::try_from(dimensions.height()).map_err(|_| resource_limit_error())?;
    let source_row_bytes = source_width
        .checked_mul(4)
        .ok_or_else(resource_limit_error)?;
    let block_width_bytes = factor.checked_mul(4).ok_or_else(resource_limit_error)?;
    let count = u64::try_from(factor)
        .ok()
        .and_then(|factor| factor.checked_mul(factor))
        .ok_or_else(resource_limit_error)?;
    let mut output = Vec::with_capacity(capacity);
    for output_y in 0..output_height {
        let source_row = output_y
            .checked_mul(factor)
            .and_then(|row| row.checked_mul(source_row_bytes))
            .ok_or_else(resource_limit_error)?;
        for output_x in 0..output_width {
            let source_column = output_x
                .checked_mul(block_width_bytes)
                .ok_or_else(resource_limit_error)?;
            let mut sums = [0_u64; 3];
            for row in 0..factor {
                let row_offset = row
                    .checked_mul(source_row_bytes)
                    .and_then(|offset| source_row.checked_add(offset))
                    .ok_or_else(resource_limit_error)?;
                for column in 0..factor {
                    let offset = column
                        .checked_mul(4)
                        .and_then(|offset| source_column.checked_add(offset))
                        .and_then(|offset| row_offset.checked_add(offset))
                        .ok_or_else(resource_limit_error)?;
                    let rgba = &frame.pixels()[offset..offset + 4];
                    for channel in 0..3 {
                        sums[channel] = sums[channel]
                            .checked_add(u64::from(linear_channel(rgba[channel])))
                            .ok_or_else(resource_limit_error)?;
                    }
                }
            }
            for sum in sums {
                let value = sum
                    .checked_add(count / 2)
                    .ok_or_else(resource_limit_error)?
                    / count;
                output.push(u16::try_from(value).map_err(|_| resource_limit_error())?);
            }
        }
    }
    Ok(NormalizedFrame {
        id: frame.id().clone(),
        timestamp: frame.timestamp(),
        dimensions,
        linear_rgb16: Arc::<[u16]>::from(output.into_boxed_slice()),
    })
}

fn normalize_frame_general<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    crop: PixelRect,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    background: [u16; 3],
    capacity: usize,
) -> Result<NormalizedFrame<F>> {
    let mut output = Vec::with_capacity(capacity);
    let factor = u32::from(scale.factor());
    for y in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            let pixel = match scale.direction {
                ScaleDirection::Identity => {
                    composited_pixel(frame, crop.x() + x, crop.y() + y, background)?
                }
                ScaleDirection::Up => composited_pixel(
                    frame,
                    crop.x() + x / factor,
                    crop.y() + y / factor,
                    background,
                )?,
                ScaleDirection::Down => downscaled_pixel(
                    frame,
                    crop.x() + x * factor,
                    crop.y() + y * factor,
                    factor,
                    background,
                )?,
            };
            output.extend_from_slice(&pixel);
        }
    }
    Ok(NormalizedFrame {
        id: frame.id().clone(),
        timestamp: frame.timestamp(),
        dimensions,
        linear_rgb16: Arc::<[u16]>::from(output.into_boxed_slice()),
    })
}

fn composited_pixel<F, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    x: u32,
    y: u32,
    background: [u16; 3],
) -> Result<[u16; 3]> {
    let width = usize::try_from(frame.dimensions().width()).map_err(|_| resource_limit_error())?;
    let index = usize::try_from(y)
        .ok()
        .and_then(|row| row.checked_mul(width))
        .and_then(|row| row.checked_add(usize::try_from(x).ok()?))
        .and_then(|pixel| pixel.checked_mul(4))
        .ok_or_else(resource_limit_error)?;
    let rgba = &frame.pixels()[index..index + 4];
    let alpha = u32::from(rgba[3]);
    let inverse_alpha = 255 - alpha;
    let mut result = [0_u16; 3];
    for channel in 0..3 {
        let source = u32::from(linear_channel(rgba[channel]));
        let backdrop = u32::from(background[channel]);
        let value = source
            .checked_mul(alpha)
            .and_then(|value| value.checked_add(backdrop * inverse_alpha))
            .and_then(|value| value.checked_add(127))
            .ok_or_else(resource_limit_error)?
            / 255;
        result[channel] = u16::try_from(value).map_err(|_| resource_limit_error())?;
    }
    Ok(result)
}

fn downscaled_pixel<F, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    source_x: u32,
    source_y: u32,
    factor: u32,
    background: [u16; 3],
) -> Result<[u16; 3]> {
    let mut sums = [0_u64; 3];
    for dy in 0..factor {
        for dx in 0..factor {
            let pixel = composited_pixel(frame, source_x + dx, source_y + dy, background)?;
            for channel in 0..3 {
                sums[channel] = sums[channel]
                    .checked_add(u64::from(pixel[channel]))
                    .ok_or_else(resource_limit_error)?;
            }
        }
    }
    let count = u64::from(factor) * u64::from(factor);
    let mut result = [0_u16; 3];
    for channel in 0..3 {
        let rounded = sums[channel]
            .checked_add(count / 2)
            .ok_or_else(resource_limit_error)?
            / count;
        result[channel] = u16::try_from(rounded).map_err(|_| resource_limit_error())?;
    }
    Ok(result)
}

fn normalization_steps(
    parameters: NormalizationParameters,
    crop: PixelRect,
    output_dimensions: PixelDimensions,
) -> Result<Vec<NormalizationStep>> {
    let mut steps = vec![
        NormalizationStep::new(
            NormalizationKind::ColorSpaceConversion,
            "srgb8-to-linear16-v1",
            make_parameters([
                ("input", ParameterValue::Text("rgba8_srgb_straight".into())),
                ("output", ParameterValue::Text("rgb16_linear_opaque".into())),
                ("transfer", ParameterValue::Text("iec-61966-2-1".into())),
            ])?,
        )?,
        NormalizationStep::new(
            NormalizationKind::AlphaCompositing,
            "straight-alpha-linear-v1",
            make_parameters([
                (
                    "background_srgb8",
                    ParameterValue::List(
                        parameters
                            .background
                            .channels()
                            .into_iter()
                            .map(|value| ParameterValue::Unsigned(u64::from(value)))
                            .collect(),
                    ),
                ),
                ("rounding", ParameterValue::Text("round_half_up".into())),
            ])?,
        )?,
    ];
    if parameters.crop.is_some() {
        steps.push(NormalizationStep::new(
            NormalizationKind::FixedCrop,
            "source-pixel-crop-v1",
            rect_parameters(crop)?,
        )?);
    }
    if !parameters.scale.is_identity() {
        let kernel = match parameters.scale.direction {
            ScaleDirection::Up => "nearest_neighbor",
            ScaleDirection::Down => "non_overlapping_box_average",
            ScaleDirection::Identity => unreachable!(),
        };
        steps.push(NormalizationStep::new(
            NormalizationKind::IntegerScaling,
            "integer-scale-v1",
            make_parameters([
                (
                    "direction",
                    ParameterValue::Text(parameters.scale.direction_name().into()),
                ),
                (
                    "factor",
                    ParameterValue::Unsigned(u64::from(parameters.scale.factor())),
                ),
                ("kernel", ParameterValue::Text(kernel.into())),
                (
                    "output_width",
                    ParameterValue::Unsigned(u64::from(output_dimensions.width())),
                ),
                (
                    "output_height",
                    ParameterValue::Unsigned(u64::from(output_dimensions.height())),
                ),
                (
                    "mask_reduction",
                    ParameterValue::Text("all_source_pixels".into()),
                ),
            ])?,
        )?);
    }
    Ok(steps)
}

fn rect_parameters(rect: PixelRect) -> Result<Parameters> {
    make_parameters([
        ("x", ParameterValue::Unsigned(u64::from(rect.x()))),
        ("y", ParameterValue::Unsigned(u64::from(rect.y()))),
        ("width", ParameterValue::Unsigned(u64::from(rect.width()))),
        ("height", ParameterValue::Unsigned(u64::from(rect.height()))),
    ])
}

pub(crate) fn make_parameters<const N: usize>(
    entries: [(&'static str, ParameterValue); N],
) -> Result<Parameters> {
    Parameters::new(
        entries
            .into_iter()
            .map(|(key, value)| (Box::<str>::from(key), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

const fn linear_channel(value: u8) -> u16 {
    SRGB8_TO_LINEAR16[value as usize]
}

/// Deterministic inverse of the checked-in transfer table. Values between table
/// entries select the nearest encoded channel; exact ties select the lower byte.
pub(crate) fn linear16_to_srgb8(value: u16) -> u8 {
    match SRGB8_TO_LINEAR16.binary_search(&value) {
        Ok(index) => index as u8,
        Err(0) => 0,
        Err(256) => 255,
        Err(upper) => {
            let lower = upper - 1;
            let lower_distance = value - SRGB8_TO_LINEAR16[lower];
            let upper_distance = SRGB8_TO_LINEAR16[upper] - value;
            if lower_distance <= upper_distance {
                lower as u8
            } else {
                upper as u8
            }
        }
    }
}

// IEC 61966-2-1 sRGB EOTF values mapped to 0..=65535 with round-half-up.
const SRGB8_TO_LINEAR16: [u16; 256] = [
    0, 20, 40, 60, 80, 99, 119, 139, 159, 179, 199, 219, 241, 264, 288, 313, 340, 367, 396, 427,
    458, 491, 526, 562, 599, 637, 677, 718, 761, 805, 851, 898, 947, 997, 1048, 1101, 1156, 1212,
    1270, 1330, 1391, 1453, 1517, 1583, 1651, 1720, 1790, 1863, 1937, 2013, 2090, 2170, 2250, 2333,
    2418, 2504, 2592, 2681, 2773, 2866, 2961, 3058, 3157, 3258, 3360, 3464, 3570, 3678, 3788, 3900,
    4014, 4129, 4247, 4366, 4488, 4611, 4736, 4864, 4993, 5124, 5257, 5392, 5530, 5669, 5810, 5953,
    6099, 6246, 6395, 6547, 6700, 6856, 7014, 7174, 7335, 7500, 7666, 7834, 8004, 8177, 8352, 8528,
    8708, 8889, 9072, 9258, 9445, 9635, 9828, 10022, 10219, 10417, 10619, 10822, 11028, 11235,
    11446, 11658, 11873, 12090, 12309, 12530, 12754, 12980, 13209, 13440, 13673, 13909, 14146,
    14387, 14629, 14874, 15122, 15371, 15623, 15878, 16135, 16394, 16656, 16920, 17187, 17456,
    17727, 18001, 18277, 18556, 18837, 19121, 19407, 19696, 19987, 20281, 20577, 20876, 21177,
    21481, 21787, 22096, 22407, 22721, 23038, 23357, 23678, 24002, 24329, 24658, 24990, 25325,
    25662, 26001, 26344, 26688, 27036, 27386, 27739, 28094, 28452, 28813, 29176, 29542, 29911,
    30282, 30656, 31033, 31412, 31794, 32179, 32567, 32957, 33350, 33745, 34143, 34544, 34948,
    35355, 35764, 36176, 36591, 37008, 37429, 37852, 38278, 38706, 39138, 39572, 40009, 40449,
    40891, 41337, 41785, 42236, 42690, 43147, 43606, 44069, 44534, 45002, 45473, 45947, 46423,
    46903, 47385, 47871, 48359, 48850, 49344, 49841, 50341, 50844, 51349, 51858, 52369, 52884,
    53401, 53921, 54445, 54971, 55500, 56032, 56567, 57105, 57646, 58190, 58737, 59287, 59840,
    60396, 60955, 61517, 62082, 62650, 63221, 63795, 64372, 64952, 65535,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArtifactLabels, DeclaredGap, FrameRegion, Marker, MeasurementParameters, PixelFormat,
        RenderLimits, StoryboardParameters, StoryboardTileLimit, generate_storyboard,
    };
    use sha2::{Digest, Sha256};

    fn frame(id: u8, dimensions: PixelDimensions, pixels: Vec<u8>) -> Frame<u8, Box<[u8]>> {
        Frame::new(
            id,
            Timestamp::from_nanos(u64::from(id)),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            pixels.into_boxed_slice(),
        )
        .unwrap()
    }

    #[test]
    fn lookup_table_is_stable() {
        assert_eq!(SRGB8_TO_LINEAR16[0], 0);
        assert_eq!(SRGB8_TO_LINEAR16[10], 199);
        assert_eq!(SRGB8_TO_LINEAR16[128], 14_146);
        assert_eq!(SRGB8_TO_LINEAR16[255], 65_535);
        assert_eq!(
            SRGB8_TO_LINEAR16
                .iter()
                .map(|value| u64::from(*value))
                .sum::<u64>(),
            5_217_863
        );
        assert_eq!(linear16_to_srgb8(0), 0);
        assert_eq!(linear16_to_srgb8(65_535), 255);
        let midpoint = (SRGB8_TO_LINEAR16[100] + SRGB8_TO_LINEAR16[101]) / 2;
        assert_eq!(linear16_to_srgb8(midpoint), 100);
    }

    #[test]
    fn opaque_full_frame_fast_path_matches_reference_across_rectangular_scales() {
        let dimensions = PixelDimensions::new(40, 24).unwrap();
        let sequence = opaque_rectangular_sequence(dimensions);

        for (factor, scale) in [
            (1, IntegerScale::IDENTITY),
            (2, IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap()),
            (4, IntegerScale::down(NonZeroU8::new(4).unwrap()).unwrap()),
            (8, IntegerScale::down(NonZeroU8::new(8).unwrap()).unwrap()),
        ] {
            let parameters = NormalizationParameters::new(
                Rgb8::new(19, 37, 73),
                None,
                scale,
                ProcessingLimits::default(),
            );
            let optimized = normalize_sequence(&sequence, parameters).unwrap();
            let crop = optimized.source_crop();
            let capacity = optimized.dimensions().pixel_count().unwrap() * 3;
            assert!(sequence.frames().iter().all(|frame| {
                can_use_opaque_full_frame_fast_path(
                    frame,
                    crop,
                    optimized.dimensions(),
                    scale,
                    true,
                )
            }));

            // Keep the old general traversal as the executable reference oracle. Comparing the
            // complete sequence catches row-stride and block-boundary errors that a tiny fixture
            // cannot expose, including the final row of the factor-eight output.
            let mut reference = optimized.clone();
            reference.frames = sequence
                .frames()
                .iter()
                .map(|frame| {
                    normalize_frame_general(
                        frame,
                        crop,
                        optimized.dimensions(),
                        scale,
                        parameters.background.channels().map(linear_channel),
                        capacity,
                    )
                })
                .collect::<Result<Vec<_>>>()
                .unwrap()
                .into_boxed_slice();
            assert_eq!(optimized, reference, "opaque scale factor {factor}");

            if factor == 4 {
                let labels = ArtifactLabels::new(
                    "Opaque normalization equivalence",
                    "rectangular generated fixture",
                )
                .unwrap();
                let request = StoryboardParameters::new(
                    Timestamp::from_nanos(2),
                    StoryboardTileLimit::new(5).unwrap(),
                    MeasurementParameters::new(0),
                    labels,
                    RenderLimits::default(),
                );
                let optimized_artifacts =
                    generate_storyboard(41_u8, Some(42_u8), &sequence, &optimized, request.clone())
                        .unwrap();
                let reference_artifacts =
                    generate_storyboard(41_u8, Some(42_u8), &sequence, &reference, request)
                        .unwrap();
                assert_eq!(optimized_artifacts, reference_artifacts);
                for (optimized_artifact, reference_artifact) in [
                    (
                        optimized_artifacts.storyboard(),
                        reference_artifacts.storyboard(),
                    ),
                    (
                        optimized_artifacts.orientation().unwrap(),
                        reference_artifacts.orientation().unwrap(),
                    ),
                ] {
                    assert_eq!(
                        &optimized_artifact.image().bytes()[..8],
                        b"\x89PNG\r\n\x1a\n"
                    );
                    assert_eq!(
                        optimized_artifact.image().bytes(),
                        reference_artifact.image().bytes()
                    );
                    assert_eq!(optimized_artifact.manifest(), reference_artifact.manifest());
                    let output_digest: [u8; 32] =
                        Sha256::digest(optimized_artifact.image().bytes()).into();
                    assert_eq!(
                        optimized_artifact.manifest().output_hash().as_bytes(),
                        &output_digest
                    );
                }
            }
        }
    }

    fn opaque_rectangular_sequence(
        dimensions: PixelDimensions,
    ) -> FrameSequence<u8, u8, u8, Box<[u8]>> {
        let frames = (0_u8..5)
            .map(|frame_index| {
                let mut pixels = vec![0_u8; dimensions.rgba8_byte_len().unwrap()];
                for y in 0..dimensions.height() {
                    for x in 0..dimensions.width() {
                        let offset = ((y * dimensions.width() + x) * 4) as usize;
                        pixels[offset..offset + 4].copy_from_slice(&[
                            (x * 13 + y * 7 + u32::from(frame_index) * 19) as u8,
                            (x * 5 + y * 17 + u32::from(frame_index) * 29 + x * y) as u8,
                            (x * 23 + y * 3 + u32::from(frame_index) * 11 + x * y * 2) as u8,
                            u8::MAX,
                        ]);
                    }
                }
                frame(frame_index, dimensions, pixels)
            })
            .collect();
        FrameSequence::new(
            frames,
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn opaque_full_frame_fast_path_selection_excludes_other_semantics() {
        let dimensions = PixelDimensions::new(2, 2).unwrap();
        let crop = PixelRect::new(0, 0, dimensions.width(), dimensions.height()).unwrap();
        let opaque = frame(
            1,
            dimensions,
            vec![
                255, 0, 0, 255, 0, 128, 255, 255, 32, 64, 96, 255, 200, 180, 160, 255,
            ],
        );
        assert!(can_use_opaque_full_frame_fast_path(
            &opaque,
            crop,
            dimensions,
            IntegerScale::IDENTITY,
            true,
        ));
        assert!(!can_use_opaque_full_frame_fast_path(
            &opaque,
            crop,
            dimensions,
            IntegerScale::IDENTITY,
            false,
        ));
        assert!(!can_use_opaque_full_frame_fast_path(
            &opaque,
            PixelRect::new(1, 0, 1, 2).unwrap(),
            PixelDimensions::new(1, 2).unwrap(),
            IntegerScale::IDENTITY,
            true,
        ));
        assert!(!can_use_opaque_full_frame_fast_path(
            &opaque,
            crop,
            dimensions,
            IntegerScale::up(NonZeroU8::new(2).unwrap()).unwrap(),
            true,
        ));
        assert!(can_use_opaque_full_frame_fast_path(
            &opaque,
            crop,
            PixelDimensions::new(1, 1).unwrap(),
            IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
            true,
        ));

        let mut alpha_pixels = opaque.pixels().to_vec();
        alpha_pixels[3] = 127;
        let alpha = frame(2, dimensions, alpha_pixels);
        assert!(!can_use_opaque_full_frame_fast_path(
            &alpha,
            crop,
            dimensions,
            IntegerScale::IDENTITY,
            true,
        ));
    }

    #[test]
    fn exact_alpha_and_scaling_kernels_are_stable() {
        let dimensions = PixelDimensions::new(2, 1).unwrap();
        let sequence = FrameSequence::new(
            vec![frame(1, dimensions, vec![255, 0, 0, 0, 255, 255, 255, 128])],
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
        )
        .unwrap();
        let normalized = normalize_sequence(
            &sequence,
            NormalizationParameters::new(
                Rgb8::new(0, 128, 0),
                None,
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        assert_eq!(
            normalized.frames()[0].linear_rgb16(),
            &[0, 14_146, 0, 32_896, 39_941, 32_896]
        );

        let upscaled = normalize_sequence(
            &sequence,
            NormalizationParameters::new(
                Rgb8::new(0, 128, 0),
                Some(PixelRect::new(1, 0, 1, 1).unwrap()),
                IntegerScale::up(NonZeroU8::new(2).unwrap()).unwrap(),
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        assert_eq!(upscaled.dimensions(), PixelDimensions::new(2, 2).unwrap());
        assert!(
            upscaled.frames()[0]
                .linear_rgb16()
                .chunks_exact(3)
                .all(|pixel| pixel == [32_896, 39_941, 32_896])
        );
    }

    #[test]
    fn downscale_requires_complete_analysis_boxes() {
        let dimensions = PixelDimensions::new(2, 2).unwrap();
        let pixels = [0, 0, 0, 255].repeat(4);
        let sequence = FrameSequence::new(
            vec![frame(1, dimensions, pixels)],
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            Some(FrameRegion::new(PixelRect::new(0, 0, 1, 2).unwrap(), dimensions).unwrap()),
            None,
        )
        .unwrap();
        let error = normalize_sequence(
            &sequence,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
                ProcessingLimits::default(),
            ),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::EmptyAnalysisDomain);
    }

    #[test]
    fn assembles_shared_normalized_sequence_without_copying_pixels() {
        let dimensions = PixelDimensions::new(2, 1).unwrap();
        let source = FrameSequence::new(
            vec![
                frame(1, dimensions, vec![10, 20, 30, 255, 40, 50, 60, 255]),
                frame(2, dimensions, vec![11, 21, 31, 255, 41, 51, 61, 255]),
            ],
            Vec::<Marker<u8>>::new(),
            vec![
                DeclaredGap::new(
                    1,
                    TimeRange::new(Timestamp::from_nanos(1), Timestamp::from_nanos(2)).unwrap(),
                    "capture gap",
                    None,
                )
                .unwrap(),
            ],
            None,
            None,
        )
        .unwrap();
        let original = normalize_sequence(
            &source,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        let shared_frames = original
            .frames()
            .iter()
            .map(|frame| {
                NormalizedFrame::new(
                    *frame.id(),
                    frame.timestamp(),
                    frame.dimensions(),
                    Arc::clone(frame.pixels()),
                )
                .unwrap()
            })
            .collect();
        let rebuilt = NormalizedSequence::from_parts(
            original.source_dimensions(),
            original.source_crop(),
            original.dimensions(),
            shared_frames,
            original.analysis_mask().cloned(),
            original.analysis_pixel_count(),
            original.gap_ranges().to_vec(),
            original.normalization_steps().to_vec(),
        )
        .unwrap();
        assert_eq!(rebuilt.frames().len(), 2);
        assert_eq!(rebuilt.dimensions(), original.dimensions());
        assert_eq!(rebuilt.source_dimensions(), original.source_dimensions());
        assert_eq!(rebuilt.source_crop(), original.source_crop());
        assert_eq!(rebuilt.gap_ranges(), original.gap_ranges());
        assert_eq!(rebuilt.analysis_mask(), original.analysis_mask());
        assert_eq!(
            rebuilt.analysis_pixel_count(),
            original.analysis_pixel_count()
        );
        assert_eq!(
            rebuilt.normalization_steps(),
            original.normalization_steps()
        );
        assert_eq!(
            rebuilt.frames()[0].linear_rgb16(),
            original.frames()[0].linear_rgb16()
        );
        assert_eq!(
            rebuilt.frames()[0].pixels().as_ptr(),
            original.frames()[0].pixels().as_ptr(),
            "shared sequence assembly must retain the immutable Arc allocation"
        );
    }

    #[test]
    fn validates_scale_arithmetic_and_retained_bytes() {
        assert_eq!(
            IntegerScale::up(NonZeroU8::new(9).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::InvalidScale
        );
        let huge = PixelRect::new(0, 0, u32::MAX / 2 + 1, 1).unwrap();
        assert_eq!(
            scaled_dimensions(huge, IntegerScale::up(NonZeroU8::new(2).unwrap()).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::InvalidScale
        );
        assert_eq!(
            validate_resource_limits(
                usize::MAX,
                usize::MAX,
                true,
                ProcessingLimits::new(NonZeroUsize::MAX, NonZeroUsize::MAX, NonZeroUsize::MAX),
            )
            .unwrap_err()
            .code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}
