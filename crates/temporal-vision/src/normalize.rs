use std::{
    collections::BTreeMap,
    mem::size_of,
    num::{NonZeroU8, NonZeroUsize},
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScaleDirection {
    Identity,
    Up,
    Down,
}

/// A bounded whole-number image scale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    fn direction_name(self) -> &'static str {
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
    linear_rgb16: Box<[u16]>,
}

impl<F> NormalizedFrame<F> {
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
}

/// Normalize one validated source geometry epoch.
pub fn normalize_sequence<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
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

    // Build and validate the domain before retaining any normalized frame buffers.
    let (analysis_mask, analysis_pixel_count) = transformed_analysis_mask(
        sequence,
        crop,
        dimensions,
        parameters.scale,
        restricted_domain,
    )?;
    let background = parameters.background.channels().map(linear_channel);
    let frames = sequence
        .frames()
        .iter()
        .map(|frame| normalize_frame(frame, crop, dimensions, parameters.scale, background))
        .collect::<Result<Vec<_>>>()?
        .into_boxed_slice();
    let gap_ranges = sequence
        .gaps()
        .iter()
        .map(|gap| gap.range())
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let normalization_steps = normalization_steps(parameters, crop, dimensions)?;

    Ok(NormalizedSequence {
        source_dimensions,
        source_crop: crop,
        dimensions,
        frames,
        analysis_mask,
        analysis_pixel_count,
        gap_ranges,
        normalization_steps: normalization_steps.into_boxed_slice(),
    })
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

fn normalize_frame<F: Clone, P: AsRef<[u8]>>(
    frame: &Frame<F, P>,
    crop: PixelRect,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    background: [u16; 3],
) -> Result<NormalizedFrame<F>> {
    let capacity = dimensions
        .pixel_count()?
        .checked_mul(3)
        .ok_or_else(resource_limit_error)?;
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
        linear_rgb16: output.into_boxed_slice(),
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
    use crate::{DeclaredGap, FrameRegion, Marker, PixelFormat};

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
