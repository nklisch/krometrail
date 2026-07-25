use std::{
    collections::BTreeMap,
    fmt::Display,
    num::{NonZeroU8, NonZeroU32, NonZeroUsize},
};

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, BinaryMask, DeclaredGap, EncodedImage,
    ErrorCode, EvidenceClass, FrameRegion, FrameSequence, GeneratedArtifact, IntegerScale, Marker,
    NormalizationKind, NormalizationParameters, NormalizationStep, ParameterValue, Parameters,
    PixelDimensions, PixelRect, ProcessingLimits, Result, Rgb8, Timestamp, VisionError,
    generator_descriptor,
    normalize::make_parameters,
    normalize_sequence,
    render::{
        canvas::{
            BLACK, Canvas, MUTED, PANEL, WARNING, WHITE, canvas_limit_error,
            canvas_output_limit_error,
        },
        font::{CELL_WIDTH, draw_text, ellipsize},
    },
};

stable_registry! {
    /// Coordinate space in which a fixed filmstrip region was declared.
    pub enum RegionCoordinateSpace {
        SourceImage => "source_image",
        Viewport => "viewport",
    }
}

/// A non-empty half-open rectangle whose origin may lie outside the source image.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct SignedPixelRect {
    x: i64,
    y: i64,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl SignedPixelRect {
    pub fn new(x: i64, y: i64, width: NonZeroU32, height: NonZeroU32) -> Result<Self> {
        let rect = Self {
            x,
            y,
            width,
            height,
        };
        rect.right_exclusive()?;
        rect.bottom_exclusive()?;
        Ok(rect)
    }

    /// Converts finite fractional bounds to the smallest containing pixel rect.
    /// Left/top round down and right/bottom round up, including below zero.
    pub fn from_outward_f64_bounds(left: f64, top: f64, right: f64, bottom: f64) -> Result<Self> {
        if ![left, top, right, bottom]
            .iter()
            .all(|value| value.is_finite())
            || right <= left
            || bottom <= top
        {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "fractional region bounds must be finite and non-empty",
            ));
        }
        const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
        let convert = |value: f64, round: fn(f64) -> f64| {
            let value = round(value);
            if value < i64::MIN as f64 || value >= I64_UPPER_EXCLUSIVE {
                Err(VisionError::new(
                    ErrorCode::InvalidRegion,
                    "fractional region exceeds the supported coordinate space",
                ))
            } else {
                Ok(value as i64)
            }
        };
        let left = convert(left, f64::floor)?;
        let top = convert(top, f64::floor)?;
        let right = convert(right, f64::ceil)?;
        let bottom = convert(bottom, f64::ceil)?;
        let width = u32::try_from(i128::from(right) - i128::from(left))
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidRegion,
                    "fractional region width exceeds the supported coordinate space",
                )
            })?;
        let height = u32::try_from(i128::from(bottom) - i128::from(top))
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidRegion,
                    "fractional region height exceeds the supported coordinate space",
                )
            })?;
        Self::new(left, top, width, height)
    }

    pub const fn x(self) -> i64 {
        self.x
    }

    pub const fn y(self) -> i64 {
        self.y
    }

    pub const fn width(self) -> u32 {
        self.width.get()
    }

    pub const fn height(self) -> u32 {
        self.height.get()
    }

    pub fn right_exclusive(self) -> Result<i64> {
        self.x
            .checked_add(i64::from(self.width.get()))
            .ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidRegion,
                    "signed region exceeds the coordinate space",
                )
            })
    }

    pub fn bottom_exclusive(self) -> Result<i64> {
        self.y
            .checked_add(i64::from(self.height.get()))
            .ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidRegion,
                    "signed region exceeds the coordinate space",
                )
            })
    }
}

impl<'de> Deserialize<'de> for SignedPixelRect {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            x: i64,
            y: i64,
            width: NonZeroU32,
            height: NonZeroU32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.x, wire.y, wire.width, wire.height).map_err(serde::de::Error::custom)
    }
}

/// A positive exact rational scale in source pixels per declared coordinate unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RationalScale {
    numerator: NonZeroU32,
    denominator: NonZeroU32,
}

impl RationalScale {
    pub const fn new(numerator: NonZeroU32, denominator: NonZeroU32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub const fn numerator(self) -> u32 {
        self.numerator.get()
    }

    pub const fn denominator(self) -> u32 {
        self.denominator.get()
    }
}

/// Caller-declared mapping from viewport units to source-image pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ViewportMapping {
    viewport_dimensions: PixelDimensions,
    scale_x: RationalScale,
    scale_y: RationalScale,
}

impl ViewportMapping {
    pub const fn new(
        viewport_dimensions: PixelDimensions,
        scale_x: RationalScale,
        scale_y: RationalScale,
    ) -> Self {
        Self {
            viewport_dimensions,
            scale_x,
            scale_y,
        }
    }

    /// Builds the canonical exact viewport-to-source rational mapping.
    pub fn for_source(
        viewport_dimensions: PixelDimensions,
        source_dimensions: PixelDimensions,
    ) -> Self {
        fn reduced(numerator: u32, denominator: u32) -> RationalScale {
            fn gcd(mut left: u32, mut right: u32) -> u32 {
                while right != 0 {
                    let remainder = left % right;
                    left = right;
                    right = remainder;
                }
                left
            }
            let divisor = gcd(numerator, denominator);
            RationalScale::new(
                NonZeroU32::new(numerator / divisor).expect("source dimension is non-zero"),
                NonZeroU32::new(denominator / divisor).expect("viewport dimension is non-zero"),
            )
        }
        Self::new(
            viewport_dimensions,
            reduced(source_dimensions.width(), viewport_dimensions.width()),
            reduced(source_dimensions.height(), viewport_dimensions.height()),
        )
    }

    pub const fn viewport_dimensions(self) -> PixelDimensions {
        self.viewport_dimensions
    }

    pub const fn scale_x(self) -> RationalScale {
        self.scale_x
    }

    pub const fn scale_y(self) -> RationalScale {
        self.scale_y
    }
}

/// One fixed region declaration. Neither variant implies logical-element tracking.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "coordinate_space", rename_all = "snake_case")]
pub enum RegionDefinition {
    FixedSourceImage {
        rect: SignedPixelRect,
    },
    FixedViewport {
        rect: SignedPixelRect,
        mapping: ViewportMapping,
    },
}

impl RegionDefinition {
    pub const fn coordinate_space(self) -> RegionCoordinateSpace {
        match self {
            Self::FixedSourceImage { .. } => RegionCoordinateSpace::SourceImage,
            Self::FixedViewport { .. } => RegionCoordinateSpace::Viewport,
        }
    }

    pub const fn rect(self) -> SignedPixelRect {
        match self {
            Self::FixedSourceImage { rect } | Self::FixedViewport { rect, .. } => rect,
        }
    }
}

/// Missing source-image edges retained as explicit padding in unscaled pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PaddingInsets {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl PaddingInsets {
    pub const fn left(self) -> u32 {
        self.left
    }

    pub const fn top(self) -> u32 {
        self.top
    }

    pub const fn right(self) -> u32 {
        self.right
    }

    pub const fn bottom(self) -> u32 {
        self.bottom
    }

    pub const fn is_empty(self) -> bool {
        self.left == 0 && self.top == 0 && self.right == 0 && self.bottom == 0
    }
}

/// Everything needed to render one chronological fixed-region tile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FilmstripTilePlan<FrameId> {
    frame_id: FrameId,
    frame_index: usize,
    timestamp: Timestamp,
    anchor_offset_nanos: i128,
    source_rect: Option<PixelRect>,
    padding: PaddingInsets,
    gap_after: bool,
}

impl<F> FilmstripTilePlan<F> {
    pub fn frame_id(&self) -> &F {
        &self.frame_id
    }

    pub const fn frame_index(&self) -> usize {
        self.frame_index
    }

    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub const fn anchor_offset_nanos(&self) -> i128 {
        self.anchor_offset_nanos
    }

    pub const fn source_rect(&self) -> Option<PixelRect> {
        self.source_rect
    }

    pub const fn padding(&self) -> PaddingInsets {
        self.padding
    }

    pub const fn gap_after(&self) -> bool {
        self.gap_after
    }
}

/// Reusable, deterministic fixed-region selection and crop plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RegionFilmstripPlan<FrameId> {
    tiles: Box<[FilmstripTilePlan<FrameId>]>,
    locator_frame_index: usize,
    coordinate_space: RegionCoordinateSpace,
    declared_region: SignedPixelRect,
    resolved_source_region: SignedPixelRect,
    tile_source_dimensions: PixelDimensions,
    omitted_frame_count: u64,
}

impl<F> RegionFilmstripPlan<F> {
    pub fn tiles(&self) -> &[FilmstripTilePlan<F>] {
        &self.tiles
    }

    pub const fn locator_frame_index(&self) -> usize {
        self.locator_frame_index
    }

    pub const fn coordinate_space(&self) -> RegionCoordinateSpace {
        self.coordinate_space
    }

    pub const fn declared_region(&self) -> SignedPixelRect {
        self.declared_region
    }

    pub const fn resolved_source_region(&self) -> SignedPixelRect {
        self.resolved_source_region
    }

    pub const fn tile_source_dimensions(&self) -> PixelDimensions {
        self.tile_source_dimensions
    }

    pub const fn omitted_frame_count(&self) -> u64 {
        self.omitted_frame_count
    }
}

/// Maximum number of crops shown in one filmstrip.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FilmstripTileLimit(NonZeroU8);

impl FilmstripTileLimit {
    pub const DEFAULT: Self = Self(NonZeroU8::new(12).expect("default is nonzero"));

    pub fn new(value: u8) -> Result<Self> {
        let value = NonZeroU8::new(value).ok_or_else(|| {
            VisionError::new(
                ErrorCode::InvalidParameter,
                "filmstrip tile limit must be between one and twenty-four",
            )
        })?;
        if value.get() > 24 {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "filmstrip tile limit must be between one and twenty-four",
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl Default for FilmstripTileLimit {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'de> Deserialize<'de> for FilmstripTileLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Resolve one fixed region and select a bounded chronological set of source frames.
pub fn plan_region_filmstrip<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    region: RegionDefinition,
    anchor: Timestamp,
    tile_limit: FilmstripTileLimit,
    locator_frame_index: Option<usize>,
) -> Result<RegionFilmstripPlan<F>> {
    if !source.range().contains(anchor) {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "filmstrip anchor lies outside the source range",
        ));
    }
    if locator_frame_index.is_some_and(|index| index >= source.frames().len()) {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "filmstrip locator frame index is outside the source sequence",
        ));
    }

    let resolved = resolve_region(region, source.dimensions())?;
    let tile_source_dimensions = PixelDimensions::new(resolved.width(), resolved.height())
        .map_err(|_| {
            VisionError::new(
                ErrorCode::ResourceLimitExceeded,
                "resolved filmstrip region exceeds supported dimensions",
            )
        })?;
    let selected_indices = select_indices(source.frames().len(), usize::from(tile_limit.get()));
    let locator_frame_index = locator_frame_index.unwrap_or_else(|| {
        selected_indices
            .iter()
            .copied()
            .find(|index| source.frames()[*index].timestamp() >= anchor)
            .unwrap_or(selected_indices[0])
    });

    let mut tiles = selected_indices
        .iter()
        .map(|index| {
            let frame = &source.frames()[*index];
            let (source_rect, padding) = intersect_region(resolved, source.dimensions())?;
            Ok(FilmstripTilePlan {
                frame_id: frame.id().clone(),
                frame_index: *index,
                timestamp: frame.timestamp(),
                anchor_offset_nanos: i128::from(frame.timestamp().as_nanos())
                    - i128::from(anchor.as_nanos()),
                source_rect,
                padding,
                gap_after: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    for tile in 0..tiles.len().saturating_sub(1) {
        tiles[tile].gap_after = source.gaps().iter().any(|gap| {
            gap.range().start() <= tiles[tile + 1].timestamp
                && gap.range().end() >= tiles[tile].timestamp
        });
    }

    let omitted_frame_count =
        u64::try_from(source.source_frame_count() - tiles.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::ResourceLimitExceeded,
                "filmstrip frame count exceeds the manifest representation",
            )
        })?;
    Ok(RegionFilmstripPlan {
        tiles: tiles.into_boxed_slice(),
        locator_frame_index,
        coordinate_space: region.coordinate_space(),
        declared_region: region.rect(),
        resolved_source_region: resolved,
        tile_source_dimensions,
        omitted_frame_count,
    })
}

fn resolve_region(region: RegionDefinition, source: PixelDimensions) -> Result<SignedPixelRect> {
    match region {
        RegionDefinition::FixedSourceImage { rect } => Ok(rect),
        RegionDefinition::FixedViewport { rect, mapping } => {
            validate_viewport_mapping(mapping, source)?;
            let left = scale_floor(rect.x(), mapping.scale_x())?;
            let top = scale_floor(rect.y(), mapping.scale_y())?;
            let right = scale_ceil(rect.right_exclusive()?, mapping.scale_x())?;
            let bottom = scale_ceil(rect.bottom_exclusive()?, mapping.scale_y())?;
            let width = u32::try_from(right.checked_sub(left).ok_or_else(invalid_scale_error)?)
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or_else(invalid_scale_error)?;
            let height = u32::try_from(bottom.checked_sub(top).ok_or_else(invalid_scale_error)?)
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or_else(invalid_scale_error)?;
            SignedPixelRect::new(left, top, width, height)
        }
    }
}

fn validate_viewport_mapping(mapping: ViewportMapping, source: PixelDimensions) -> Result<()> {
    let mapped_width = scale_ceil(
        i64::from(mapping.viewport_dimensions().width()),
        mapping.scale_x(),
    )?;
    let mapped_height = scale_ceil(
        i64::from(mapping.viewport_dimensions().height()),
        mapping.scale_y(),
    )?;
    if mapped_width != i64::from(source.width()) || mapped_height != i64::from(source.height()) {
        return Err(VisionError::new(
            ErrorCode::InvalidScale,
            "viewport mapping dimensions contradict the source-frame dimensions",
        ));
    }
    Ok(())
}

fn scale_floor(value: i64, scale: RationalScale) -> Result<i64> {
    let numerator = i128::from(value)
        .checked_mul(i128::from(scale.numerator()))
        .ok_or_else(invalid_scale_error)?;
    let result = numerator.div_euclid(i128::from(scale.denominator()));
    i64::try_from(result).map_err(|_| invalid_scale_error())
}

fn scale_ceil(value: i64, scale: RationalScale) -> Result<i64> {
    let numerator = i128::from(value)
        .checked_mul(i128::from(scale.numerator()))
        .ok_or_else(invalid_scale_error)?;
    let denominator = i128::from(scale.denominator());
    let result = -(-numerator).div_euclid(denominator);
    i64::try_from(result).map_err(|_| invalid_scale_error())
}

fn invalid_scale_error() -> VisionError {
    VisionError::new(
        ErrorCode::InvalidScale,
        "viewport scale conversion exceeds the supported coordinate space",
    )
}

/// Select a deterministic, evenly spaced chronological subset including both endpoints.
pub fn select_indices(frame_count: usize, limit: usize) -> Vec<usize> {
    if frame_count <= limit {
        return (0..frame_count).collect();
    }
    if limit == 1 {
        return vec![0];
    }
    let span = frame_count - 1;
    let denominator = limit - 1;
    (0..limit)
        .map(|slot| {
            let numerator = (slot as u128) * (span as u128);
            round_ratio_ties_down(numerator, denominator as u128) as usize
        })
        .collect()
}

fn round_ratio_ties_down(numerator: u128, denominator: u128) -> u128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    quotient + u128::from(remainder > denominator - remainder)
}

fn intersect_region(
    region: SignedPixelRect,
    dimensions: PixelDimensions,
) -> Result<(Option<PixelRect>, PaddingInsets)> {
    let right = region.right_exclusive()?;
    let bottom = region.bottom_exclusive()?;
    let source_right = i64::from(dimensions.width());
    let source_bottom = i64::from(dimensions.height());
    let left = region.x().clamp(0, source_right);
    let top = region.y().clamp(0, source_bottom);
    let clipped_right = right.clamp(0, source_right);
    let clipped_bottom = bottom.clamp(0, source_bottom);
    let visible_width = u32::try_from(clipped_right.saturating_sub(left)).map_err(|_| {
        VisionError::new(
            ErrorCode::InvalidRegion,
            "filmstrip intersection exceeds limits",
        )
    })?;
    let visible_height = u32::try_from(clipped_bottom.saturating_sub(top)).map_err(|_| {
        VisionError::new(
            ErrorCode::InvalidRegion,
            "filmstrip intersection exceeds limits",
        )
    })?;
    let padding = PaddingInsets {
        left: u32::try_from((left - region.x()).clamp(0, i64::from(region.width())))
            .map_err(|_| invalid_region_error())?,
        top: u32::try_from((top - region.y()).clamp(0, i64::from(region.height())))
            .map_err(|_| invalid_region_error())?,
        right: region
            .width()
            .checked_sub(
                u32::try_from((clipped_right - region.x()).clamp(0, i64::from(region.width())))
                    .map_err(|_| invalid_region_error())?,
            )
            .ok_or_else(invalid_region_error)?,
        bottom: region
            .height()
            .checked_sub(
                u32::try_from((clipped_bottom - region.y()).clamp(0, i64::from(region.height())))
                    .map_err(|_| invalid_region_error())?,
            )
            .ok_or_else(invalid_region_error)?,
    };
    let source_rect = if visible_width == 0 || visible_height == 0 {
        None
    } else {
        Some(PixelRect::new(
            u32::try_from(left).map_err(|_| invalid_region_error())?,
            u32::try_from(top).map_err(|_| invalid_region_error())?,
            visible_width,
            visible_height,
        )?)
    };
    Ok((source_rect, padding))
}

fn invalid_region_error() -> VisionError {
    VisionError::new(
        ErrorCode::InvalidRegion,
        "filmstrip region intersection exceeds supported limits",
    )
}

const DEFAULT_MAX_DIMENSION: u32 = 4_096;
const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_SOURCE_FRAMES: usize = 4_096;
const MARGIN: u32 = 12;
const HEADER_HEIGHT: u32 = 64;
const LOCATOR_WIDTH: u32 = 200;
const LOCATOR_HEIGHT: u32 = 200;
const LOCATOR_ANNOTATION_HEIGHT: u32 = 44;
const TILE_ANNOTATION_HEIGHT: u32 = 56;
const MINIMUM_TILE_SLOT_WIDTH: u32 = 160;
const SECTION_GAP: u32 = 16;
const TILE_GAP: u32 = 12;
const TIMELINE_HEIGHT: u32 = 24;

/// Required title and source context drawn onto a region filmstrip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionFilmstripLabels {
    title: String,
    source: String,
}

impl RegionFilmstripLabels {
    pub fn new(title: impl Into<String>, source: impl Into<String>) -> Result<Self> {
        let title = title.into();
        let source = source.into();
        if title.trim().is_empty() || source.trim().is_empty() {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "filmstrip title and source context must not be empty",
            ));
        }
        Ok(Self { title, source })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Processing and output ceilings for one region-filmstrip generation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionFilmstripRenderLimits {
    max_width: NonZeroU32,
    max_height: NonZeroU32,
    max_canvas_bytes: NonZeroUsize,
    max_encoded_bytes: NonZeroUsize,
    max_source_frames: NonZeroUsize,
}

impl RegionFilmstripRenderLimits {
    pub const fn new(
        max_width: NonZeroU32,
        max_height: NonZeroU32,
        max_canvas_bytes: NonZeroUsize,
        max_encoded_bytes: NonZeroUsize,
    ) -> Self {
        Self {
            max_width,
            max_height,
            max_canvas_bytes,
            max_encoded_bytes,
            max_source_frames: NonZeroUsize::new(DEFAULT_MAX_SOURCE_FRAMES)
                .expect("default is nonzero"),
        }
    }

    pub const fn with_max_source_frames(mut self, max_source_frames: NonZeroUsize) -> Self {
        self.max_source_frames = max_source_frames;
        self
    }

    pub const fn max_width(self) -> u32 {
        self.max_width.get()
    }

    pub const fn max_height(self) -> u32 {
        self.max_height.get()
    }

    pub const fn max_canvas_bytes(self) -> usize {
        self.max_canvas_bytes.get()
    }

    pub const fn max_encoded_bytes(self) -> usize {
        self.max_encoded_bytes.get()
    }

    pub const fn max_source_frames(self) -> usize {
        self.max_source_frames.get()
    }

    fn processing_limits(self) -> ProcessingLimits {
        let max_pixels = (self.max_canvas_bytes() / 6).max(1);
        ProcessingLimits::new(
            self.max_source_frames,
            NonZeroUsize::new(max_pixels).expect("maximum pixels is nonzero"),
            self.max_canvas_bytes,
        )
    }
}

impl Default for RegionFilmstripRenderLimits {
    fn default() -> Self {
        Self::new(
            NonZeroU32::new(DEFAULT_MAX_DIMENSION).expect("default is nonzero"),
            NonZeroU32::new(DEFAULT_MAX_DIMENSION).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).expect("default is nonzero"),
        )
    }
}

/// Complete deterministic request for one fixed-region filmstrip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionFilmstripParameters {
    region: RegionDefinition,
    tracking_label: Option<String>,
    mask: Option<BinaryMask>,
    anchor: Timestamp,
    tile_limit: FilmstripTileLimit,
    locator_frame_index: Option<usize>,
    background: Rgb8,
    padding_color: Rgb8,
    display_scale: IntegerScale,
    labels: RegionFilmstripLabels,
    limits: RegionFilmstripRenderLimits,
}

impl RegionFilmstripParameters {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        region: RegionDefinition,
        anchor: Timestamp,
        tile_limit: FilmstripTileLimit,
        background: Rgb8,
        padding_color: Rgb8,
        display_scale: IntegerScale,
        labels: RegionFilmstripLabels,
        limits: RegionFilmstripRenderLimits,
    ) -> Self {
        Self {
            region,
            tracking_label: None,
            mask: None,
            anchor,
            tile_limit,
            locator_frame_index: None,
            background,
            padding_color,
            display_scale,
            labels,
            limits,
        }
    }

    pub const fn with_locator_frame_index(mut self, index: usize) -> Self {
        self.locator_frame_index = Some(index);
        self
    }

    /// Applies one immutable full-frame mask at identical source coordinates.
    pub fn with_mask(mut self, mask: BinaryMask) -> Result<Self> {
        if mask.bounds()?.is_none() {
            return Err(VisionError::new(
                ErrorCode::InvalidMask,
                "filmstrip mask must select at least one source pixel",
            ));
        }
        self.mask = Some(mask);
        Ok(self)
    }

    pub fn with_tracking_label(mut self, label: impl Into<String>) -> Self {
        self.tracking_label = Some(label.into());
        self
    }

    pub const fn region(&self) -> RegionDefinition {
        self.region
    }

    pub const fn mask(&self) -> Option<&BinaryMask> {
        self.mask.as_ref()
    }

    pub const fn anchor(&self) -> Timestamp {
        self.anchor
    }

    pub const fn tile_limit(&self) -> FilmstripTileLimit {
        self.tile_limit
    }

    pub const fn locator_frame_index(&self) -> Option<usize> {
        self.locator_frame_index
    }

    pub const fn background(&self) -> Rgb8 {
        self.background
    }

    pub const fn padding_color(&self) -> Rgb8 {
        self.padding_color
    }

    pub const fn display_scale(&self) -> IntegerScale {
        self.display_scale
    }

    pub const fn labels(&self) -> &RegionFilmstripLabels {
        &self.labels
    }

    pub const fn limits(&self) -> RegionFilmstripRenderLimits {
        self.limits
    }
}

/// Encoded filmstrip evidence, provenance, and its reusable fixed-region plan.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionFilmstripArtifact<A, F, M, G> {
    artifact: GeneratedArtifact<A, F, M, G>,
    plan: RegionFilmstripPlan<F>,
}

impl<A, F, M, G> RegionFilmstripArtifact<A, F, M, G> {
    pub const fn image(&self) -> &EncodedImage {
        self.artifact.image()
    }

    pub const fn manifest(&self) -> &ArtifactManifest<A, F, M, G> {
        self.artifact.manifest()
    }

    pub const fn plan(&self) -> &RegionFilmstripPlan<F> {
        &self.plan
    }
}

/// Generate one deterministic source-derived fixed-region filmstrip.
pub fn generate_region_filmstrip<A, F, M, G, P>(
    artifact_id: A,
    source: &FrameSequence<F, M, G, P>,
    parameters: RegionFilmstripParameters,
) -> Result<RegionFilmstripArtifact<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    if source.frames().len() > parameters.limits.max_source_frames() {
        return Err(render_limit_error());
    }
    let effective_region = if let Some(mask) = parameters.mask.as_ref() {
        if mask.dimensions() != source.dimensions() {
            return Err(VisionError::new(
                ErrorCode::InvalidMask,
                "filmstrip mask dimensions must match every source frame",
            ));
        }
        let bounds = mask.bounds()?.ok_or_else(|| {
            VisionError::new(
                ErrorCode::InvalidMask,
                "filmstrip mask must select at least one source pixel",
            )
        })?;
        RegionDefinition::FixedSourceImage {
            rect: SignedPixelRect::new(
                i64::from(bounds.x()),
                i64::from(bounds.y()),
                NonZeroU32::new(bounds.width()).expect("mask bounds are non-empty"),
                NonZeroU32::new(bounds.height()).expect("mask bounds are non-empty"),
            )?,
        }
    } else {
        parameters.region
    };
    let plan = plan_region_filmstrip(
        source,
        effective_region,
        parameters.anchor,
        parameters.tile_limit,
        parameters.locator_frame_index,
    )?;
    let tile_dimensions =
        scaled_tile_dimensions(plan.tile_source_dimensions(), parameters.display_scale)?;
    let (crop, _) = intersect_region(effective_region.rect(), source.dimensions())?;
    // A fully outside region is rendered entirely as padding. Keep normalization bounded to a
    // single source pixel because draw_tile never reads normalized data for that tile.
    let crop = crop.unwrap_or(PixelRect::new(0, 0, 1, 1)?);
    let tile_source = FrameSequence::new(
        plan.tiles()
            .iter()
            .map(|tile| source.frames()[tile.frame_index()].to_owned())
            .collect(),
        Vec::<Marker<M>>::new(),
        Vec::<DeclaredGap<G>>::new(),
        source.region(),
        source.mask().cloned(),
    )?;
    let normalized = normalize_sequence(
        &tile_source,
        NormalizationParameters::new(
            parameters.background,
            Some(crop),
            IntegerScale::IDENTITY,
            parameters.limits.processing_limits(),
        ),
    )?;
    let locator_source = FrameSequence::new(
        vec![source.frames()[plan.locator_frame_index()].to_owned()],
        Vec::<Marker<M>>::new(),
        Vec::<DeclaredGap<G>>::new(),
        None,
        None,
    )?;
    let locator_normalized = normalize_sequence(
        &locator_source,
        NormalizationParameters::new(
            parameters.background,
            None,
            IntegerScale::IDENTITY,
            parameters.limits.processing_limits(),
        ),
    )?;
    let layout = FilmstripLayout::new(tile_dimensions, plan.tiles().len(), parameters.limits)?;
    let mut canvas = Canvas::new(
        layout.dimensions,
        BLACK,
        parameters.limits.max_canvas_bytes(),
    )?;
    render_filmstrip(
        &mut canvas,
        layout,
        source,
        &normalized,
        &locator_normalized,
        &plan,
        &parameters,
    )?;
    let (bytes, hash) = crate::encode::encode_png(
        layout.dimensions,
        canvas.pixels(),
        parameters.limits.max_encoded_bytes(),
    )?;

    let mut normalization = normalized.normalization_steps().to_vec();
    normalization.push(display_conversion_step()?);
    normalization.push(region_padding_step(&plan, parameters.padding_color)?);
    if let Some(mask) = parameters.mask.as_ref() {
        normalization.push(mask_application_step(mask, &plan)?);
    }
    if !parameters.display_scale.is_identity() {
        normalization.push(display_scale_step(
            parameters.display_scale,
            tile_dimensions,
        )?);
    }
    let mut artifact_source_indices = plan
        .tiles()
        .iter()
        .map(FilmstripTilePlan::frame_index)
        .collect::<Vec<_>>();
    if !artifact_source_indices.contains(&plan.locator_frame_index()) {
        artifact_source_indices.push(plan.locator_frame_index());
        artifact_source_indices.sort_unstable();
    }
    let selected_ids = artifact_source_indices
        .iter()
        .map(|index| source.frames()[*index].id().clone())
        .collect();
    let manifest_region = manifest_region(effective_region, source.dimensions())?;
    let manifest = ArtifactManifest::from_sequence_with_domain(
        artifact_id,
        ArtifactKind::RegionFilmstrip,
        EvidenceClass::SourceDerived,
        {
            let descriptor = generator_descriptor(ArtifactKind::RegionFilmstrip);
            AlgorithmDescriptor::new(descriptor.name, descriptor.version)?
        },
        source,
        manifest_region,
        parameters.mask.clone(),
        selected_ids,
        // A filmstrip looks at the frames it renders. Frames between tiles are
        // decoded by the plan and then never examined, so counting them as
        // analyzed would overstate what this artifact is evidence of.
        crate::provenance::SequenceConsumption::SelectedFramesOnly,
        normalization,
        filmstrip_parameters(
            &plan,
            &artifact_source_indices,
            source.source_frame_count(),
            &parameters,
            layout,
            tile_dimensions,
        )?,
        layout.dimensions,
        hash,
    )?;
    Ok(RegionFilmstripArtifact {
        artifact: GeneratedArtifact::new(EncodedImage::new(layout.dimensions, bytes), manifest),
        plan,
    })
}

/// A fixed-size crop location for one source frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TrackedRegion {
    pub frame_index: usize,
    pub rect: SignedPixelRect,
}

/// Parameters for a filmstrip whose crop follows a recorded region per frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackedFilmstripParameters {
    pub regions: Vec<TrackedRegion>,
    pub anchor: Timestamp,
    pub tile_limit: FilmstripTileLimit,
    pub background: Rgb8,
    pub padding_color: Rgb8,
    pub display_scale: IntegerScale,
    pub labels: RegionFilmstripLabels,
    pub limits: RegionFilmstripRenderLimits,
}

impl TrackedFilmstripParameters {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        regions: Vec<TrackedRegion>,
        anchor: Timestamp,
        tile_limit: FilmstripTileLimit,
        background: Rgb8,
        padding_color: Rgb8,
        display_scale: IntegerScale,
        labels: RegionFilmstripLabels,
        limits: RegionFilmstripRenderLimits,
    ) -> Self {
        Self {
            regions,
            anchor,
            tile_limit,
            background,
            padding_color,
            display_scale,
            labels,
            limits,
        }
    }
}

/// Generate a per-frame moving crop. All tiles share the first union crop for
/// normalization, while their plans retain their own source rectangle/padding.
pub fn generate_tracked_region_filmstrip<A, F, M, G, P>(
    artifact_id: A,
    source: &FrameSequence<F, M, G, P>,
    parameters: TrackedFilmstripParameters,
) -> Result<RegionFilmstripArtifact<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    if parameters.regions.is_empty() {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "tracked filmstrip requires regions",
        ));
    }
    let selected = select_indices(
        source.frames().len(),
        usize::from(parameters.tile_limit.get()),
    );
    let mut by_frame = std::collections::BTreeMap::new();
    for region in parameters.regions {
        by_frame.insert(region.frame_index, region.rect);
    }
    let first = by_frame.values().next().copied().ok_or_else(|| {
        VisionError::new(
            ErrorCode::InvalidParameter,
            "tracked filmstrip requires regions",
        )
    })?;
    let mut union = first;
    for rect in by_frame.values().copied() {
        if rect.width() != first.width() || rect.height() != first.height() {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "tracked crop dimensions must remain fixed",
            ));
        }
        let left = union.x().min(rect.x());
        let top = union.y().min(rect.y());
        let right = union.right_exclusive()?.max(rect.right_exclusive()?);
        let bottom = union.bottom_exclusive()?.max(rect.bottom_exclusive()?);
        union = SignedPixelRect::from_outward_f64_bounds(
            left as f64,
            top as f64,
            right as f64,
            bottom as f64,
        )?;
    }
    let tile_dims = PixelDimensions::new(first.width(), first.height())?;
    let mut tiles = Vec::with_capacity(selected.len());
    for index in &selected {
        let frame = &source.frames()[*index];
        let Some(rect) = by_frame.get(index).copied() else {
            tiles.push(FilmstripTilePlan {
                frame_id: frame.id().clone(),
                frame_index: *index,
                timestamp: frame.timestamp(),
                anchor_offset_nanos: i128::from(frame.timestamp().as_nanos())
                    - i128::from(parameters.anchor.as_nanos()),
                source_rect: None,
                padding: PaddingInsets {
                    left: first.width(),
                    top: first.height(),
                    right: first.width(),
                    bottom: first.height(),
                },
                gap_after: false,
            });
            continue;
        };
        let (source_rect, padding) = intersect_region(rect, source.dimensions())?;
        tiles.push(FilmstripTilePlan {
            frame_id: frame.id().clone(),
            frame_index: *index,
            timestamp: frame.timestamp(),
            anchor_offset_nanos: i128::from(frame.timestamp().as_nanos())
                - i128::from(parameters.anchor.as_nanos()),
            source_rect,
            padding,
            gap_after: false,
        });
    }
    let plan = RegionFilmstripPlan {
        tiles: tiles.into_boxed_slice(),
        locator_frame_index: selected[0],
        coordinate_space: RegionCoordinateSpace::SourceImage,
        declared_region: union,
        resolved_source_region: union,
        tile_source_dimensions: tile_dims,
        omitted_frame_count: (source.frames().len() - selected.len()) as u64,
    };
    let tile_source = FrameSequence::new(
        selected
            .iter()
            .map(|i| source.frames()[*i].to_owned())
            .collect(),
        Vec::<Marker<M>>::new(),
        Vec::<DeclaredGap<G>>::new(),
        source.region(),
        source.mask().cloned(),
    )?;
    let normalized = normalize_sequence(
        &tile_source,
        NormalizationParameters::new(
            parameters.background,
            Some(
                intersect_region(union, source.dimensions())?
                    .0
                    .unwrap_or(PixelRect::new(0, 0, 1, 1)?),
            ),
            IntegerScale::IDENTITY,
            parameters.limits.processing_limits(),
        ),
    )?;
    let locator_source = FrameSequence::new(
        vec![source.frames()[selected[0]].to_owned()],
        Vec::<Marker<M>>::new(),
        Vec::<DeclaredGap<G>>::new(),
        None,
        None,
    )?;
    let locator = normalize_sequence(
        &locator_source,
        NormalizationParameters::new(
            parameters.background,
            None,
            IntegerScale::IDENTITY,
            parameters.limits.processing_limits(),
        ),
    )?;
    let display_dims = scaled_tile_dimensions(tile_dims, parameters.display_scale)?;
    let layout = FilmstripLayout::new(display_dims, selected.len(), parameters.limits)?;
    let mut canvas = Canvas::new(
        layout.dimensions,
        BLACK,
        parameters.limits.max_canvas_bytes(),
    )?;
    let fixed = RegionFilmstripParameters::new(
        RegionDefinition::FixedSourceImage { rect: union },
        parameters.anchor,
        parameters.tile_limit,
        parameters.background,
        parameters.padding_color,
        parameters.display_scale,
        parameters.labels,
        parameters.limits,
    )
    .with_tracking_label("TRACKING NODE | PER-FRAME REGION");
    render_filmstrip(
        &mut canvas,
        layout,
        source,
        &normalized,
        &locator,
        &plan,
        &fixed,
    )?;
    let (bytes, hash) = crate::encode::encode_png(
        layout.dimensions,
        canvas.pixels(),
        parameters.limits.max_encoded_bytes(),
    )?;
    let manifest = ArtifactManifest::from_sequence_with_domain(
        artifact_id,
        ArtifactKind::RegionFilmstrip,
        EvidenceClass::SourceDerived,
        {
            let d = generator_descriptor(ArtifactKind::RegionFilmstrip);
            AlgorithmDescriptor::new(d.name, d.version)?
        },
        source,
        manifest_region(
            RegionDefinition::FixedSourceImage { rect: union },
            source.dimensions(),
        )?,
        None,
        selected
            .iter()
            .map(|i| source.frames()[*i].id().clone())
            .collect(),
        crate::provenance::SequenceConsumption::SelectedFramesOnly,
        Vec::new(),
        Parameters::new(BTreeMap::new())?,
        layout.dimensions,
        hash,
    )?;
    Ok(RegionFilmstripArtifact {
        artifact: GeneratedArtifact::new(EncodedImage::new(layout.dimensions, bytes), manifest),
        plan,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FilmstripLayout {
    dimensions: PixelDimensions,
    locator_panel: PixelRect,
    locator_annotation: PixelRect,
    strip_x: u32,
    strip_y: u32,
    tile_slot_width: u32,
    tile_width: u32,
    tile_height: u32,
    columns: usize,
    rows: usize,
    timeline_y: u32,
}

impl FilmstripLayout {
    fn new(
        tile: PixelDimensions,
        tile_count: usize,
        limits: RegionFilmstripRenderLimits,
    ) -> Result<Self> {
        let tile_slot_width = tile.width().max(MINIMUM_TILE_SLOT_WIDTH);
        let fixed_width = MARGIN
            .checked_mul(2)
            .and_then(|value| value.checked_add(LOCATOR_WIDTH))
            .and_then(|value| value.checked_add(SECTION_GAP))
            .ok_or_else(canvas_limit_error)?;
        let available = limits
            .max_width()
            .checked_sub(fixed_width)
            .ok_or_else(canvas_limit_error)?;
        let columns = usize::try_from(
            available
                .checked_add(TILE_GAP)
                .ok_or_else(canvas_limit_error)?
                / tile_slot_width
                    .checked_add(TILE_GAP)
                    .ok_or_else(canvas_limit_error)?,
        )
        .map_err(|_| canvas_limit_error())?
        .min(tile_count);
        if columns == 0 {
            return Err(canvas_limit_error());
        }
        let rows = tile_count.div_ceil(columns);
        let columns_u32 = u32::try_from(columns).map_err(|_| canvas_limit_error())?;
        let rows_u32 = u32::try_from(rows).map_err(|_| canvas_limit_error())?;
        let strip_width = columns_u32
            .checked_mul(tile_slot_width)
            .and_then(|value| {
                value.checked_add(TILE_GAP.checked_mul(columns_u32.saturating_sub(1))?)
            })
            .ok_or_else(canvas_limit_error)?;
        let row_height = tile
            .height()
            .checked_add(TILE_ANNOTATION_HEIGHT)
            .ok_or_else(canvas_limit_error)?;
        let strip_height = rows_u32
            .checked_mul(row_height)
            .and_then(|value| value.checked_add(TILE_GAP.checked_mul(rows_u32 - 1)?))
            .ok_or_else(canvas_limit_error)?;
        let locator_height = LOCATOR_HEIGHT
            .checked_add(LOCATOR_ANNOTATION_HEIGHT)
            .ok_or_else(canvas_limit_error)?;
        let content_height = strip_height.max(locator_height);
        let width = fixed_width
            .checked_add(strip_width)
            .ok_or_else(canvas_limit_error)?;
        let timeline_y = HEADER_HEIGHT
            .checked_add(MARGIN)
            .and_then(|value| value.checked_add(content_height))
            .and_then(|value| value.checked_add(MARGIN))
            .ok_or_else(canvas_limit_error)?;
        let height = timeline_y
            .checked_add(TIMELINE_HEIGHT)
            .ok_or_else(canvas_limit_error)?;
        if width > limits.max_width() || height > limits.max_height() {
            return Err(canvas_output_limit_error(
                width,
                height,
                limits.max_width(),
                limits.max_height(),
            ));
        }
        let dimensions = PixelDimensions::new(width, height).map_err(|_| canvas_limit_error())?;
        let bytes = dimensions
            .pixel_count()?
            .checked_mul(3)
            .ok_or_else(canvas_limit_error)?;
        if bytes > limits.max_canvas_bytes() {
            return Err(canvas_limit_error());
        }
        let locator_y = HEADER_HEIGHT + MARGIN;
        Ok(Self {
            dimensions,
            locator_panel: PixelRect::new(MARGIN, locator_y, LOCATOR_WIDTH, LOCATOR_HEIGHT)?,
            locator_annotation: PixelRect::new(
                MARGIN,
                locator_y + LOCATOR_HEIGHT,
                LOCATOR_WIDTH,
                LOCATOR_ANNOTATION_HEIGHT,
            )?,
            strip_x: MARGIN + LOCATOR_WIDTH + SECTION_GAP,
            strip_y: locator_y,
            tile_slot_width,
            tile_width: tile.width(),
            tile_height: tile.height(),
            columns,
            rows,
            timeline_y,
        })
    }

    fn tile_slot(self, index: usize) -> Result<PixelRect> {
        let column = u32::try_from(index % self.columns).map_err(|_| canvas_limit_error())?;
        let row = u32::try_from(index / self.columns).map_err(|_| canvas_limit_error())?;
        let x = self
            .strip_x
            .checked_add(
                column
                    .checked_mul(self.tile_slot_width + TILE_GAP)
                    .ok_or_else(canvas_limit_error)?,
            )
            .ok_or_else(canvas_limit_error)?;
        let y = self
            .strip_y
            .checked_add(
                row.checked_mul(self.tile_height + TILE_ANNOTATION_HEIGHT + TILE_GAP)
                    .ok_or_else(canvas_limit_error)?,
            )
            .ok_or_else(canvas_limit_error)?;
        PixelRect::new(
            x,
            y,
            self.tile_slot_width,
            self.tile_height + TILE_ANNOTATION_HEIGHT,
        )
    }
}

fn scaled_tile_dimensions(source: PixelDimensions, scale: IntegerScale) -> Result<PixelDimensions> {
    let factor = u32::from(scale.factor());
    let (width, height) = match scale.direction_name() {
        "identity" => (source.width(), source.height()),
        "up" => (
            source
                .width()
                .checked_mul(factor)
                .ok_or_else(invalid_scale_error)?,
            source
                .height()
                .checked_mul(factor)
                .ok_or_else(invalid_scale_error)?,
        ),
        "down" => {
            if source.width() % factor != 0 || source.height() % factor != 0 {
                return Err(VisionError::new(
                    ErrorCode::InvalidScale,
                    "filmstrip downscale factor must exactly divide declared region dimensions",
                ));
            }
            (source.width() / factor, source.height() / factor)
        }
        _ => unreachable!("integer scale direction is a closed internal registry"),
    };
    PixelDimensions::new(width, height).map_err(|_| invalid_scale_error())
}

fn render_filmstrip<F: Display + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    layout: FilmstripLayout,
    source: &FrameSequence<F, M, G, P>,
    normalized: &crate::NormalizedSequence<F>,
    locator_normalized: &crate::NormalizedSequence<F>,
    plan: &RegionFilmstripPlan<F>,
    parameters: &RegionFilmstripParameters,
) -> Result<()> {
    draw_header(canvas, source, plan, parameters)?;
    draw_locator(
        canvas,
        layout,
        &locator_normalized.frames()[0],
        source.dimensions(),
        plan,
    )?;
    for (index, tile) in plan.tiles().iter().enumerate() {
        draw_tile(
            canvas,
            layout,
            index,
            tile,
            &normalized.frames()[index],
            normalized.source_crop(),
            parameters,
        )?;
    }
    draw_timeline(canvas, layout, source)
}

fn draw_header<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    source: &FrameSequence<F, M, G, P>,
    plan: &RegionFilmstripPlan<F>,
    parameters: &RegionFilmstripParameters,
) -> Result<()> {
    canvas.fill_rect(0, 0, canvas.dimensions().width(), HEADER_HEIGHT, BLACK)?;
    let width = canvas.dimensions().width().saturating_sub(8);
    draw_clipped_text(canvas, 4, 2, width, parameters.labels.title(), WHITE)?;
    draw_clipped_text(canvas, 4, 14, width, parameters.labels.source(), MUTED)?;
    draw_clipped_text(
        canvas,
        4,
        26,
        width,
        &format!(
            "RANGE {} - {} | ANCHOR {}",
            format_time(source.range().start()),
            format_time(source.range().end()),
            format_time(parameters.anchor)
        ),
        MUTED,
    )?;
    let status = if source.gaps().is_empty() {
        if let Some(label) = parameters.tracking_label.as_deref() {
            format!(
                "SOURCE-DERIVED | PER-FRAME REGION | {label} | STRIP OMITTED {}",
                plan.omitted_frame_count()
            )
        } else {
            format!(
                "SOURCE-DERIVED | FIXED {} | TRACKING NONE | STRIP OMITTED {}",
                plan.coordinate_space().as_str(),
                plan.omitted_frame_count()
            )
        }
    } else if let Some(label) = parameters.tracking_label.as_deref() {
        format!("GAP - UNSEEN BEHAVIOR MAY HAVE OCCURRED | PER-FRAME REGION | {label}")
    } else {
        format!(
            "GAP - UNSEEN BEHAVIOR MAY HAVE OCCURRED | FIXED {} | TRACKING NONE",
            plan.coordinate_space().as_str()
        )
    };
    draw_clipped_text(
        canvas,
        4,
        38,
        width,
        &status,
        if source.gaps().is_empty() {
            MUTED
        } else {
            WARNING
        },
    )?;
    draw_clipped_text(
        canvas,
        4,
        50,
        width,
        if parameters.mask.is_some() {
            "MASK APPLIED | SELECTED PIXELS SHOWN | EXCLUDED PIXELS HATCHED"
        } else {
            parameters
                .tracking_label
                .as_deref()
                .unwrap_or("FIXED VISUAL REGION; NO LOGICAL ELEMENT FOLLOWING")
        },
        if parameters.mask.is_some() {
            WARNING
        } else {
            MUTED
        },
    )
}

fn draw_locator<F>(
    canvas: &mut Canvas,
    layout: FilmstripLayout,
    frame: &crate::NormalizedFrame<F>,
    source_dimensions: PixelDimensions,
    plan: &RegionFilmstripPlan<F>,
) -> Result<()> {
    canvas.fill_rect(
        layout.locator_panel.x(),
        layout.locator_panel.y(),
        layout.locator_panel.width(),
        layout.locator_panel.height(),
        PANEL,
    )?;
    let (draw_x, draw_y, draw_width, draw_height) = canvas.draw_linear_frame(
        frame.dimensions(),
        frame.linear_rgb16(),
        layout.locator_panel.x(),
        layout.locator_panel.y(),
        layout.locator_panel.width(),
        layout.locator_panel.height(),
    )?;
    let (intersection, _) = intersect_region(plan.resolved_source_region(), source_dimensions)?;
    if let Some(rect) = intersection {
        let x0 = draw_x + map_floor(rect.x(), draw_width, source_dimensions.width())?;
        let y0 = draw_y + map_floor(rect.y(), draw_height, source_dimensions.height())?;
        let x1 = draw_x
            + map_ceil(
                rect.right_exclusive()?,
                draw_width,
                source_dimensions.width(),
            )?;
        let y1 = draw_y
            + map_ceil(
                rect.bottom_exclusive()?,
                draw_height,
                source_dimensions.height(),
            )?;
        draw_outline(
            canvas,
            x0,
            y0,
            x1.saturating_sub(x0),
            y1.saturating_sub(y0),
            WARNING,
        )?;
    }
    let resolved = plan.resolved_source_region();
    let right = resolved.right_exclusive()?;
    let bottom = resolved.bottom_exclusive()?;
    let mut outside = Vec::new();
    if resolved.x() < 0 {
        draw_edge_chevrons(canvas, draw_x, draw_y, draw_width, draw_height, Edge::Left)?;
        outside.push("LEFT");
    }
    if right > i64::from(source_dimensions.width()) {
        draw_edge_chevrons(canvas, draw_x, draw_y, draw_width, draw_height, Edge::Right)?;
        outside.push("RIGHT");
    }
    if resolved.y() < 0 {
        draw_edge_chevrons(canvas, draw_x, draw_y, draw_width, draw_height, Edge::Top)?;
        outside.push("TOP");
    }
    if bottom > i64::from(source_dimensions.height()) {
        draw_edge_chevrons(
            canvas,
            draw_x,
            draw_y,
            draw_width,
            draw_height,
            Edge::Bottom,
        )?;
        outside.push("BOTTOM");
    }
    canvas.fill_rect(
        layout.locator_annotation.x(),
        layout.locator_annotation.y(),
        layout.locator_annotation.width(),
        layout.locator_annotation.height(),
        PANEL,
    )?;
    draw_clipped_text(
        canvas,
        layout.locator_annotation.x() + 3,
        layout.locator_annotation.y() + 3,
        layout.locator_annotation.width().saturating_sub(6),
        "REGION IN CONTEXT",
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        layout.locator_annotation.x() + 3,
        layout.locator_annotation.y() + 15,
        layout.locator_annotation.width().saturating_sub(6),
        &format!("LOCATOR FRAME {}", plan.locator_frame_index()),
        MUTED,
    )?;
    let outside_label = if outside.is_empty() {
        "REGION INSIDE SOURCE".to_owned()
    } else {
        format!("OUTSIDE: {}", outside.join(" + "))
    };
    draw_clipped_text(
        canvas,
        layout.locator_annotation.x() + 3,
        layout.locator_annotation.y() + 27,
        layout.locator_annotation.width().saturating_sub(6),
        &outside_label,
        if outside.is_empty() { MUTED } else { WARNING },
    )
}

fn draw_tile<F: Display>(
    canvas: &mut Canvas,
    layout: FilmstripLayout,
    index: usize,
    tile: &FilmstripTilePlan<F>,
    frame: &crate::NormalizedFrame<F>,
    crop: PixelRect,
    parameters: &RegionFilmstripParameters,
) -> Result<()> {
    let slot = layout.tile_slot(index)?;
    let image_x = slot.x() + (slot.width() - layout.tile_width) / 2;
    canvas.fill_rect(slot.x(), slot.y(), slot.width(), layout.tile_height, PANEL)?;
    for y in 0..layout.tile_height {
        for x in 0..layout.tile_width {
            let (color, padding) = scaled_region_pixel(
                frame,
                tile,
                parameters.mask.as_ref(),
                parameters.padding_color.channels(),
                parameters.display_scale,
                crop,
                x,
                y,
            )?;
            let color = if padding && (x + y) % 8 < 2 {
                WARNING
            } else {
                color
            };
            canvas.set_pixel(image_x + x, slot.y() + y, color)?;
        }
    }
    let annotation_y = slot.y() + layout.tile_height;
    canvas.fill_rect(
        slot.x(),
        annotation_y,
        slot.width(),
        TILE_ANNOTATION_HEIGHT,
        PANEL,
    )?;
    let text_width = slot.width().saturating_sub(6);
    draw_clipped_text(
        canvas,
        slot.x() + 3,
        annotation_y + 2,
        text_width,
        &format!(
            "T {} | {}",
            format_time(tile.timestamp()),
            format_offset(tile.anchor_offset_nanos())
        ),
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        slot.x() + 3,
        annotation_y + 14,
        text_width,
        &format!(
            "FRAME {} | SOURCE INDEX {}",
            tile.frame_id(),
            tile.frame_index()
        ),
        MUTED,
    )?;
    let padding = tile.padding();
    let padding_label = if tile.source_rect().is_none() {
        "OUTSIDE SOURCE | ALL PADDING".to_owned()
    } else if padding.is_empty() {
        "SOURCE CROP | NO PADDING".to_owned()
    } else {
        format!(
            "PADDING L{} T{} R{} B{}",
            padding.left(),
            padding.top(),
            padding.right(),
            padding.bottom()
        )
    };
    draw_clipped_text(
        canvas,
        slot.x() + 3,
        annotation_y + 26,
        text_width,
        &padding_label,
        if padding.is_empty() { MUTED } else { WARNING },
    )?;
    draw_clipped_text(
        canvas,
        slot.x() + 3,
        annotation_y + 38,
        text_width,
        if parameters.mask.is_some() {
            "FIXED MASK | TRACKING NONE"
        } else if parameters.tracking_label.is_some() {
            "TRACKED REGION | PER-FRAME CROP"
        } else {
            "FIXED REGION | TRACKING NONE"
        },
        if parameters.mask.is_some() {
            WARNING
        } else {
            MUTED
        },
    )?;
    if tile.gap_after() {
        let hatch_x = slot.right_exclusive()?.saturating_sub(6);
        canvas.draw_hatch(hatch_x, slot.y(), 6, slot.height(), WARNING)?;
        draw_clipped_text(
            canvas,
            slot.x() + 3,
            annotation_y + 44,
            text_width,
            "GAP ->",
            WARNING,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scaled_region_pixel<F>(
    frame: &crate::NormalizedFrame<F>,
    tile: &FilmstripTilePlan<F>,
    mask: Option<&BinaryMask>,
    padding_color: [u8; 3],
    scale: IntegerScale,
    crop: PixelRect,
    output_x: u32,
    output_y: u32,
) -> Result<([u8; 3], bool)> {
    let factor = u32::from(scale.factor());
    if scale.direction_name() != "down" {
        let source_x = if scale.direction_name() == "up" {
            output_x / factor
        } else {
            output_x
        };
        let source_y = if scale.direction_name() == "up" {
            output_y / factor
        } else {
            output_y
        };
        return region_pixel(frame, tile, mask, padding_color, crop, source_x, source_y);
    }

    let mut sums = [0_u64; 3];
    let mut padding = false;
    for dy in 0..factor {
        for dx in 0..factor {
            let (pixel, generated) = region_pixel(
                frame,
                tile,
                mask,
                padding_color,
                crop,
                output_x * factor + dx,
                output_y * factor + dy,
            )?;
            padding |= generated;
            for channel in 0..3 {
                sums[channel] += u64::from(pixel[channel]);
            }
        }
    }
    let count = u64::from(factor) * u64::from(factor);
    Ok((
        std::array::from_fn(|channel| ((sums[channel] + count / 2) / count) as u8),
        padding,
    ))
}

fn region_pixel<F>(
    frame: &crate::NormalizedFrame<F>,
    tile: &FilmstripTilePlan<F>,
    mask: Option<&BinaryMask>,
    padding_color: [u8; 3],
    crop: PixelRect,
    x: u32,
    y: u32,
) -> Result<([u8; 3], bool)> {
    let padding = tile.padding();
    let Some(source_rect) = tile.source_rect() else {
        return Ok((padding_color, true));
    };
    let visible_x = x.checked_sub(padding.left());
    let visible_y = y.checked_sub(padding.top());
    if visible_x.is_none_or(|value| value >= source_rect.width())
        || visible_y.is_none_or(|value| value >= source_rect.height())
    {
        return Ok((padding_color, true));
    }
    let source_x = source_rect.x() + visible_x.expect("checked above");
    let source_y = source_rect.y() + visible_y.expect("checked above");
    if mask.is_some_and(|mask| mask.includes(source_x, source_y) != Some(true)) {
        return Ok((padding_color, true));
    }
    let index = usize::try_from(source_y.saturating_sub(crop.y()))
        .ok()
        .and_then(|row| row.checked_mul(usize::try_from(frame.dimensions().width()).ok()?))
        .and_then(|row| row.checked_add(usize::try_from(source_x.saturating_sub(crop.x())).ok()?))
        .and_then(|pixel| pixel.checked_mul(3))
        .ok_or_else(canvas_limit_error)?;
    let linear = &frame.linear_rgb16()[index..index + 3];
    Ok((
        [linear[0], linear[1], linear[2]].map(crate::normalize::linear16_to_srgb8),
        false,
    ))
}

fn draw_timeline<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    layout: FilmstripLayout,
    source: &FrameSequence<F, M, G, P>,
) -> Result<()> {
    canvas.fill_rect(
        0,
        layout.timeline_y,
        layout.dimensions.width(),
        TIMELINE_HEIGHT,
        BLACK,
    )?;
    draw_clipped_text(
        canvas,
        4,
        layout.timeline_y + 6,
        layout.dimensions.width().saturating_sub(8),
        &format!(
            "TIME -> {} -> {} | CHRONOLOGICAL SOURCE ORDER | ROWS {}",
            format_time(source.range().start()),
            format_time(source.range().end()),
            layout.rows
        ),
        WHITE,
    )
}

#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

fn draw_edge_chevrons(
    canvas: &mut Canvas,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    edge: Edge,
) -> Result<()> {
    for offset in (4..match edge {
        Edge::Left | Edge::Right => height,
        _ => width,
    })
        .step_by(12)
    {
        for arm in 0..4 {
            let (px, py) = match edge {
                Edge::Left => (x + arm, y + offset.saturating_sub(2) + arm),
                Edge::Right => (x + width - 1 - arm, y + offset.saturating_sub(2) + arm),
                Edge::Top => (x + offset.saturating_sub(2) + arm, y + arm),
                Edge::Bottom => (x + offset.saturating_sub(2) + arm, y + height - 1 - arm),
            };
            if px < x + width && py < y + height {
                canvas.set_pixel(px, py, WARNING)?;
            }
        }
    }
    Ok(())
}

fn draw_outline(
    canvas: &mut Canvas,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: [u8; 3],
) -> Result<()> {
    if width == 0 || height == 0 {
        return Ok(());
    }
    canvas.fill_rect(x, y, width, height.min(2), color)?;
    canvas.fill_rect(x, y + height.saturating_sub(2), width, height.min(2), color)?;
    canvas.fill_rect(x, y, width.min(2), height, color)?;
    canvas.fill_rect(x + width.saturating_sub(2), y, width.min(2), height, color)
}

fn map_floor(value: u32, target: u32, source: u32) -> Result<u32> {
    u32::try_from(u64::from(value) * u64::from(target) / u64::from(source))
        .map_err(|_| canvas_limit_error())
}

fn map_ceil(value: u32, target: u32, source: u32) -> Result<u32> {
    let numerator = u64::from(value)
        .checked_mul(u64::from(target))
        .and_then(|value| value.checked_add(u64::from(source) - 1))
        .ok_or_else(canvas_limit_error)?;
    u32::try_from(numerator / u64::from(source)).map_err(|_| canvas_limit_error())
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

fn format_time(timestamp: Timestamp) -> String {
    let milliseconds = timestamp.as_nanos() / 1_000_000;
    let micros = timestamp.as_nanos() % 1_000_000 / 1_000;
    format!("{milliseconds}.{micros:03} MS")
}

fn format_offset(offset: i128) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let magnitude = offset.unsigned_abs();
    let milliseconds = magnitude / 1_000_000;
    let micros = magnitude % 1_000_000 / 1_000;
    format!("{sign}{milliseconds}.{micros:03} MS")
}

fn display_conversion_step() -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::ColorSpaceConversion,
        "linear16-to-srgb8-v1",
        make_parameters([
            ("input", ParameterValue::Text("rgb16_linear_opaque".into())),
            ("output", ParameterValue::Text("rgb8_srgb_opaque".into())),
            (
                "mapping",
                ParameterValue::Text("nearest_checked_lut_entry".into()),
            ),
        ])?,
    )
}

fn region_padding_step<F>(
    plan: &RegionFilmstripPlan<F>,
    padding_color: Rgb8,
) -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::FixedCrop,
        "fixed-region-padding-v1",
        make_parameters([
            (
                "resolved_source_region",
                signed_rect_value(plan.resolved_source_region())?,
            ),
            ("padding_rgb8", rgb_value(padding_color)),
            (
                "missing_pixels",
                ParameterValue::Text("explicit_padding_with_warning_hatch".into()),
            ),
        ])?,
    )
}

fn mask_application_step<F>(
    mask: &BinaryMask,
    plan: &RegionFilmstripPlan<F>,
) -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::FixedCrop,
        "fixed-binary-mask-v1",
        make_parameters([
            ("mask_dimensions", dimensions_value(mask.dimensions())?),
            (
                "mask_bounds",
                signed_rect_value(plan.resolved_source_region())?,
            ),
            (
                "excluded_pixels",
                ParameterValue::Text("padding_color_with_warning_hatch".into()),
            ),
            (
                "mask_sha256",
                ParameterValue::Text(mask_sha256(mask).into()),
            ),
        ])?,
    )
}

fn display_scale_step(
    scale: IntegerScale,
    dimensions: PixelDimensions,
) -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::IntegerScaling,
        "filmstrip-display-scale-v1",
        make_parameters([
            (
                "direction",
                ParameterValue::Text(scale.direction_name().into()),
            ),
            (
                "factor",
                ParameterValue::Unsigned(u64::from(scale.factor())),
            ),
            (
                "kernel",
                ParameterValue::Text(
                    if scale.direction_name() == "down" {
                        "non_overlapping_srgb8_box_average"
                    } else {
                        "nearest_neighbor"
                    }
                    .into(),
                ),
            ),
            ("output_dimensions", dimensions_value(dimensions)?),
        ])?,
    )
}

fn manifest_region(
    region: RegionDefinition,
    dimensions: PixelDimensions,
) -> Result<Option<FrameRegion>> {
    let RegionDefinition::FixedSourceImage { rect } = region else {
        return Ok(None);
    };
    if rect.x() < 0 || rect.y() < 0 {
        return Ok(None);
    }
    let pixel_rect = PixelRect::new(
        u32::try_from(rect.x()).map_err(|_| invalid_region_error())?,
        u32::try_from(rect.y()).map_err(|_| invalid_region_error())?,
        rect.width(),
        rect.height(),
    )?;
    if !pixel_rect.fits_within(dimensions) {
        return Ok(None);
    }
    Ok(Some(FrameRegion::new(pixel_rect, dimensions)?))
}

fn filmstrip_parameters<F: Display>(
    plan: &RegionFilmstripPlan<F>,
    artifact_source_indices: &[usize],
    source_frame_count: usize,
    request: &RegionFilmstripParameters,
    layout: FilmstripLayout,
    tile_dimensions: PixelDimensions,
) -> Result<Parameters> {
    let selected = plan
        .tiles()
        .iter()
        .map(|tile| {
            object([
                ("frame_index", unsigned_usize(tile.frame_index())?),
                (
                    "frame_label",
                    ParameterValue::Text(tile.frame_id().to_string().into()),
                ),
                (
                    "timestamp_nanos",
                    ParameterValue::Unsigned(tile.timestamp().as_nanos()),
                ),
                (
                    "anchor_offset",
                    signed_offset_value(tile.anchor_offset_nanos())?,
                ),
                (
                    "selection_reason",
                    ParameterValue::Text(
                        if plan.omitted_frame_count() == 0 {
                            "all_source_frames"
                        } else {
                            "uniform_source_order_coverage"
                        }
                        .into(),
                    ),
                ),
                ("source_rect", optional_rect_value(tile.source_rect())?),
                ("padding", padding_value(tile.padding())?),
                ("gap_after", ParameterValue::Bool(tile.gap_after())),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let gap_warning_count = plan.tiles().iter().filter(|tile| tile.gap_after()).count();
    let mapping = match (request.mask.as_ref(), request.region) {
        (None, RegionDefinition::FixedViewport { mapping, .. }) => object([
            (
                "viewport_dimensions",
                dimensions_value(mapping.viewport_dimensions())?,
            ),
            ("scale_x", rational_value(mapping.scale_x())?),
            ("scale_y", rational_value(mapping.scale_y())?),
        ])?,
        _ => ParameterValue::Text("not_applicable".into()),
    };
    let effective_region = if request.mask.is_some() {
        RegionDefinition::FixedSourceImage {
            rect: plan.resolved_source_region(),
        }
    } else {
        request.region
    };
    let mask = request.mask.as_ref().map_or_else(
        || Ok(ParameterValue::Text("none".into())),
        |mask| {
            object([
                ("dimensions", dimensions_value(mask.dimensions())?),
                ("bounds", signed_rect_value(plan.resolved_source_region())?),
                ("sha256", ParameterValue::Text(mask_sha256(mask).into())),
                (
                    "encoding",
                    ParameterValue::Text("row_major_msb_first_one_bit".into()),
                ),
            ])
        },
    )?;
    Parameters::new(
        [
            (
                "algorithm_version".into(),
                ParameterValue::Text(
                    generator_descriptor(ArtifactKind::RegionFilmstrip)
                        .version
                        .into(),
                ),
            ),
            ("region_definition".into(), region_value(effective_region)?),
            ("mask".into(), mask),
            (
                "coordinate_space".into(),
                ParameterValue::Text(plan.coordinate_space().as_str().into()),
            ),
            ("viewport_mapping".into(), mapping),
            (
                "resolved_source_region".into(),
                signed_rect_value(plan.resolved_source_region())?,
            ),
            ("selected".into(), ParameterValue::List(selected)),
            (
                "artifact_source_indices".into(),
                ParameterValue::List(
                    artifact_source_indices
                        .iter()
                        .map(|index| unsigned_usize(*index))
                        .collect::<Result<Vec<_>>>()?,
                ),
            ),
            (
                "omitted_frame_count".into(),
                unsigned_usize(
                    source_frame_count
                        .checked_sub(artifact_source_indices.len())
                        .ok_or_else(render_limit_error)?,
                )?,
            ),
            (
                "strip_omitted_frame_count".into(),
                ParameterValue::Unsigned(plan.omitted_frame_count()),
            ),
            (
                "locator_frame_index".into(),
                unsigned_usize(plan.locator_frame_index())?,
            ),
            ("background_rgb8".into(), rgb_value(request.background)),
            ("padding_rgb8".into(), rgb_value(request.padding_color)),
            (
                "display_scale".into(),
                object([
                    (
                        "direction",
                        ParameterValue::Text(request.display_scale.direction_name().into()),
                    ),
                    (
                        "factor",
                        ParameterValue::Unsigned(u64::from(request.display_scale.factor())),
                    ),
                ])?,
            ),
            (
                "tile_source_dimensions".into(),
                dimensions_value(plan.tile_source_dimensions())?,
            ),
            (
                "tile_output_dimensions".into(),
                dimensions_value(tile_dimensions)?,
            ),
            (
                "gap_warning_count".into(),
                unsigned_usize(gap_warning_count)?,
            ),
            (
                "title".into(),
                ParameterValue::Text(request.labels.title().into()),
            ),
            (
                "source_context".into(),
                ParameterValue::Text(request.labels.source().into()),
            ),
            (
                "tracking_method".into(),
                ParameterValue::Text("none".into()),
            ),
            (
                "fixed_region_semantics".into(),
                ParameterValue::Text("no_logical_element_following".into()),
            ),
            (
                "label_truncation".into(),
                ParameterValue::Text("embedded_ascii_ellipsis_exact_text_in_manifest".into()),
            ),
            (
                "output_layout".into(),
                object([
                    (
                        "name",
                        ParameterValue::Text("locator_and_wrapped_strip_v1".into()),
                    ),
                    ("columns", unsigned_usize(layout.columns)?),
                    ("rows", unsigned_usize(layout.rows)?),
                    (
                        "tile_slot_width",
                        ParameterValue::Unsigned(u64::from(layout.tile_slot_width)),
                    ),
                    (
                        "time_direction",
                        ParameterValue::Text("left_to_right_then_next_row".into()),
                    ),
                ])?,
            ),
            (
                "png".into(),
                ParameterValue::Text("png-0.17.16-rgb8-best-no_filter-no_chunks".into()),
            ),
            (
                "max_output_width".into(),
                ParameterValue::Unsigned(u64::from(request.limits.max_width())),
            ),
            (
                "max_output_height".into(),
                ParameterValue::Unsigned(u64::from(request.limits.max_height())),
            ),
            (
                "max_canvas_bytes".into(),
                unsigned_usize(request.limits.max_canvas_bytes())?,
            ),
            (
                "max_encoded_bytes".into(),
                unsigned_usize(request.limits.max_encoded_bytes())?,
            ),
            (
                "max_source_frames".into(),
                unsigned_usize(request.limits.max_source_frames())?,
            ),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>(),
    )
}

fn region_value(region: RegionDefinition) -> Result<ParameterValue> {
    match region {
        RegionDefinition::FixedSourceImage { rect } => object([
            ("kind", ParameterValue::Text("fixed_source_image".into())),
            ("rect", signed_rect_value(rect)?),
        ]),
        RegionDefinition::FixedViewport { rect, mapping } => object([
            ("kind", ParameterValue::Text("fixed_viewport".into())),
            ("rect", signed_rect_value(rect)?),
            (
                "mapping",
                object([
                    (
                        "viewport_dimensions",
                        dimensions_value(mapping.viewport_dimensions())?,
                    ),
                    ("scale_x", rational_value(mapping.scale_x())?),
                    ("scale_y", rational_value(mapping.scale_y())?),
                ])?,
            ),
        ]),
    }
}

fn signed_rect_value(rect: SignedPixelRect) -> Result<ParameterValue> {
    object([
        ("x", ParameterValue::Signed(rect.x())),
        ("y", ParameterValue::Signed(rect.y())),
        ("width", ParameterValue::Unsigned(u64::from(rect.width()))),
        ("height", ParameterValue::Unsigned(u64::from(rect.height()))),
    ])
}

fn optional_rect_value(rect: Option<PixelRect>) -> Result<ParameterValue> {
    rect.map_or_else(
        || Ok(ParameterValue::Text("none".into())),
        |rect| {
            object([
                ("x", ParameterValue::Unsigned(u64::from(rect.x()))),
                ("y", ParameterValue::Unsigned(u64::from(rect.y()))),
                ("width", ParameterValue::Unsigned(u64::from(rect.width()))),
                ("height", ParameterValue::Unsigned(u64::from(rect.height()))),
            ])
        },
    )
}

fn padding_value(padding: PaddingInsets) -> Result<ParameterValue> {
    object([
        ("left", ParameterValue::Unsigned(u64::from(padding.left()))),
        ("top", ParameterValue::Unsigned(u64::from(padding.top()))),
        (
            "right",
            ParameterValue::Unsigned(u64::from(padding.right())),
        ),
        (
            "bottom",
            ParameterValue::Unsigned(u64::from(padding.bottom())),
        ),
    ])
}

fn rational_value(scale: RationalScale) -> Result<ParameterValue> {
    object([
        (
            "numerator",
            ParameterValue::Unsigned(u64::from(scale.numerator())),
        ),
        (
            "denominator",
            ParameterValue::Unsigned(u64::from(scale.denominator())),
        ),
    ])
}

fn dimensions_value(dimensions: PixelDimensions) -> Result<ParameterValue> {
    object([
        (
            "width",
            ParameterValue::Unsigned(u64::from(dimensions.width())),
        ),
        (
            "height",
            ParameterValue::Unsigned(u64::from(dimensions.height())),
        ),
    ])
}

fn mask_sha256(mask: &BinaryMask) -> String {
    let mut digest = Sha256::new();
    digest.update(mask.dimensions().width().to_be_bytes());
    digest.update(mask.dimensions().height().to_be_bytes());
    digest.update(mask.bits());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn rgb_value(color: Rgb8) -> ParameterValue {
    ParameterValue::List(
        color
            .channels()
            .into_iter()
            .map(|channel| ParameterValue::Unsigned(u64::from(channel)))
            .collect(),
    )
}

fn signed_offset_value(value: i128) -> Result<ParameterValue> {
    let sign = if value < 0 { "negative" } else { "nonnegative" };
    let magnitude = u64::try_from(value.unsigned_abs()).map_err(|_| render_limit_error())?;
    object([
        ("sign", ParameterValue::Text(sign.into())),
        ("magnitude_nanos", ParameterValue::Unsigned(magnitude)),
    ])
}

fn unsigned_usize(value: usize) -> Result<ParameterValue> {
    u64::try_from(value)
        .map(ParameterValue::Unsigned)
        .map_err(|_| render_limit_error())
}

fn object<const N: usize>(entries: [(&'static str, ParameterValue); N]) -> Result<ParameterValue> {
    let values = entries
        .into_iter()
        .map(|(key, value)| (Box::<str>::from(key), value))
        .collect::<BTreeMap<_, _>>();
    Parameters::new(values.clone())?;
    Ok(ParameterValue::Object(values))
}

fn render_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "region filmstrip exceeds configured processing or rendering limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeclaredGap, Frame, Marker, PixelFormat};

    fn source(frame_count: u8) -> FrameSequence<u8, u8, u8, Box<[u8]>> {
        let dimensions = PixelDimensions::new(8, 6).unwrap();
        FrameSequence::new(
            (0..frame_count)
                .map(|id| {
                    Frame::new(
                        id,
                        Timestamp::from_nanos(u64::from(id) * 10),
                        dimensions,
                        PixelFormat::Rgba8SrgbStraight,
                        vec![0; dimensions.rgba8_byte_len().unwrap()].into_boxed_slice(),
                    )
                    .unwrap()
                })
                .collect(),
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn resolves_outward_viewport_mapping_and_rejects_contradictions() {
        let mapping = ViewportMapping::new(
            PixelDimensions::new(4, 3).unwrap(),
            RationalScale::new(NonZeroU32::new(2).unwrap(), NonZeroU32::new(1).unwrap()),
            RationalScale::new(NonZeroU32::new(2).unwrap(), NonZeroU32::new(1).unwrap()),
        );
        let rect = SignedPixelRect::new(
            -1,
            1,
            NonZeroU32::new(3).unwrap(),
            NonZeroU32::new(2).unwrap(),
        )
        .unwrap();
        let plan = plan_region_filmstrip(
            &source(2),
            RegionDefinition::FixedViewport { rect, mapping },
            Timestamp::ZERO,
            FilmstripTileLimit::DEFAULT,
            None,
        )
        .unwrap();
        assert_eq!(plan.resolved_source_region().x(), -2);
        assert_eq!(plan.resolved_source_region().width(), 6);
        assert_eq!(plan.tiles()[0].padding().left(), 2);
        assert_eq!(plan.tiles()[0].source_rect().unwrap().width(), 4);

        let wrong = ViewportMapping::new(
            PixelDimensions::new(5, 3).unwrap(),
            mapping.scale_x(),
            mapping.scale_y(),
        );
        assert_eq!(
            plan_region_filmstrip(
                &source(2),
                RegionDefinition::FixedViewport {
                    rect,
                    mapping: wrong,
                },
                Timestamp::ZERO,
                FilmstripTileLimit::DEFAULT,
                None,
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidScale
        );
    }

    #[test]
    fn exact_padding_and_selection_cover_partial_and_fully_outside_regions() {
        let partial = SignedPixelRect::new(
            -2,
            -1,
            NonZeroU32::new(12).unwrap(),
            NonZeroU32::new(9).unwrap(),
        )
        .unwrap();
        let plan = plan_region_filmstrip(
            &source(10),
            RegionDefinition::FixedSourceImage { rect: partial },
            Timestamp::from_nanos(50),
            FilmstripTileLimit::new(4).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            plan.tiles()
                .iter()
                .map(FilmstripTilePlan::frame_index)
                .collect::<Vec<_>>(),
            [0, 3, 6, 9]
        );
        assert_eq!(plan.locator_frame_index(), 6);
        assert_eq!(plan.omitted_frame_count(), 6);
        let tile = &plan.tiles()[0];
        assert_eq!(
            tile.source_rect(),
            Some(PixelRect::new(0, 0, 8, 6).unwrap())
        );
        assert_eq!(
            tile.padding(),
            PaddingInsets {
                left: 2,
                top: 1,
                right: 2,
                bottom: 2
            }
        );

        let outside = SignedPixelRect::new(
            20,
            20,
            NonZeroU32::new(3).unwrap(),
            NonZeroU32::new(2).unwrap(),
        )
        .unwrap();
        let plan = plan_region_filmstrip(
            &source(1),
            RegionDefinition::FixedSourceImage { rect: outside },
            Timestamp::ZERO,
            FilmstripTileLimit::DEFAULT,
            None,
        )
        .unwrap();
        assert_eq!(plan.tiles()[0].source_rect(), None);
        assert_eq!(
            plan.tiles()[0].padding(),
            PaddingInsets {
                left: 0,
                top: 0,
                right: 3,
                bottom: 2
            }
        );
    }

    #[test]
    fn selected_tile_subsequence_matches_full_normalization() {
        let source = source(10);
        let region = SignedPixelRect::new(
            0,
            0,
            NonZeroU32::new(8).unwrap(),
            NonZeroU32::new(6).unwrap(),
        )
        .unwrap();
        let plan = plan_region_filmstrip(
            &source,
            RegionDefinition::FixedSourceImage { rect: region },
            Timestamp::from_nanos(50),
            FilmstripTileLimit::new(4).unwrap(),
            None,
        )
        .unwrap();
        let tile_source = FrameSequence::new(
            plan.tiles()
                .iter()
                .map(|tile| source.frames()[tile.frame_index()].clone())
                .collect(),
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
        )
        .unwrap();
        let limits = ProcessingLimits::default();
        let parameters = |sequence: &FrameSequence<u8, u8, u8, Box<[u8]>>| {
            normalize_sequence(
                sequence,
                NormalizationParameters::new(
                    Rgb8::new(0, 0, 0),
                    Some(PixelRect::new(0, 0, 8, 6).unwrap()),
                    IntegerScale::IDENTITY,
                    limits,
                ),
            )
            .unwrap()
        };
        let full = parameters(&source);
        let selected = parameters(&tile_source);
        for (position, tile) in plan.tiles().iter().enumerate() {
            assert_eq!(
                selected.frames()[position],
                full.frames()[tile.frame_index()]
            );
        }
    }

    #[test]
    fn filmstrip_120_frames_fit_when_only_selected_tiles_are_retained() {
        let dimensions = PixelDimensions::new(128, 128).unwrap();
        let source = sized_source(120, dimensions);
        let region = SignedPixelRect::new(
            0,
            0,
            NonZeroU32::new(128).unwrap(),
            NonZeroU32::new(128).unwrap(),
        )
        .unwrap();
        let limits = RegionFilmstripRenderLimits::new(
            NonZeroU32::new(4_096).unwrap(),
            NonZeroU32::new(4_096).unwrap(),
            NonZeroUsize::new(1_000_000).unwrap(),
            NonZeroUsize::new(1_000_000).unwrap(),
        )
        .with_max_source_frames(NonZeroUsize::new(120).unwrap());
        let artifact = generate_region_filmstrip(
            1_u32,
            &source,
            RegionFilmstripParameters::new(
                RegionDefinition::FixedSourceImage { rect: region },
                Timestamp::from_nanos(400),
                FilmstripTileLimit::new(3).unwrap(),
                Rgb8::new(0, 0, 0),
                Rgb8::new(255, 0, 255),
                IntegerScale::IDENTITY,
                RegionFilmstripLabels::new("FILMSTRIP", "SOURCE").unwrap(),
                limits,
            ),
        )
        .unwrap();
        assert_eq!(artifact.plan().tiles().len(), 3);
        assert!(!artifact.image().bytes().is_empty());
    }

    fn sized_source(
        frame_count: usize,
        dimensions: PixelDimensions,
    ) -> FrameSequence<u32, u32, u32, Box<[u8]>> {
        FrameSequence::new(
            (0..frame_count)
                .map(|id| {
                    Frame::new(
                        u32::try_from(id).unwrap(),
                        Timestamp::from_nanos(u64::try_from(id).unwrap() * 10),
                        dimensions,
                        PixelFormat::Rgba8SrgbStraight,
                        vec![0; dimensions.rgba8_byte_len().unwrap()].into_boxed_slice(),
                    )
                    .unwrap()
                })
                .collect(),
            Vec::<Marker<u32>>::new(),
            Vec::<DeclaredGap<u32>>::new(),
            None,
            None,
        )
        .unwrap()
    }

    fn tracked_params(regions: Vec<TrackedRegion>, tile_limit: u8) -> TrackedFilmstripParameters {
        TrackedFilmstripParameters::new(
            regions,
            Timestamp::ZERO,
            FilmstripTileLimit::new(tile_limit).unwrap(),
            Rgb8::new(0, 0, 0),
            Rgb8::new(32, 32, 32),
            IntegerScale::IDENTITY,
            RegionFilmstripLabels::new("TRACKED".to_owned(), "TEST".to_owned()).unwrap(),
            RegionFilmstripRenderLimits::default(),
        )
    }

    fn rect(x: i64, y: i64, w: u32, h: u32) -> SignedPixelRect {
        SignedPixelRect::new(
            x,
            y,
            NonZeroU32::new(w).unwrap(),
            NonZeroU32::new(h).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn tracked_filmstrip_crops_each_tile_from_its_own_region() {
        // 4 frames at 8x6; two 4x3 regions at opposite corners. Evenly-spaced
        // selection over 4 frames with limit 4 selects all frames, but only
        // frames 1 and 3 have regions.
        let seq = sized_source(4, PixelDimensions::new(8, 6).unwrap());
        let artifact = generate_tracked_region_filmstrip(
            "tracked",
            &seq,
            tracked_params(
                vec![
                    TrackedRegion {
                        frame_index: 1,
                        rect: rect(0, 0, 4, 3),
                    },
                    TrackedRegion {
                        frame_index: 3,
                        rect: rect(4, 3, 4, 3),
                    },
                ],
                4,
            ),
        )
        .unwrap();
        let tiles = artifact.plan().tiles();
        assert_eq!(tiles.len(), 4);
        // Frame 1: top-left region, fully inside the source.
        let tile1 = &tiles[1];
        assert_eq!(tile1.source_rect().unwrap().x(), 0);
        assert_eq!(tile1.source_rect().unwrap().y(), 0);
        assert!(tile1.padding().is_empty());
        // Frame 3: bottom-right region, fully inside the source.
        let tile3 = &tiles[3];
        assert_eq!(tile3.source_rect().unwrap().x(), 4);
        assert_eq!(tile3.source_rect().unwrap().y(), 3);
        assert!(tile3.padding().is_empty());
        assert!(!artifact.image().bytes().is_empty());
    }

    #[test]
    fn tracked_filmstrip_missing_selected_frame_renders_padded_tile() {
        // Regions only cover frames 0 and 3; the selected middle frames get
        // fully-padded tiles instead of an error.
        let seq = sized_source(4, PixelDimensions::new(8, 6).unwrap());
        let artifact = generate_tracked_region_filmstrip(
            "tracked",
            &seq,
            tracked_params(
                vec![
                    TrackedRegion {
                        frame_index: 0,
                        rect: rect(0, 0, 4, 3),
                    },
                    TrackedRegion {
                        frame_index: 3,
                        rect: rect(0, 0, 4, 3),
                    },
                ],
                4,
            ),
        )
        .unwrap();
        let tiles = artifact.plan().tiles();
        assert_eq!(tiles.len(), 4);
        for index in [1usize, 2] {
            let tile = &tiles[index];
            assert!(
                tile.source_rect().is_none(),
                "frame {index} without a region must be fully padded"
            );
            assert_eq!(tile.padding().left(), 4);
            assert_eq!(tile.padding().top(), 3);
        }
        assert!(!artifact.image().bytes().is_empty());
    }

    #[test]
    fn tracked_filmstrip_is_deterministic() {
        let seq = sized_source(3, PixelDimensions::new(8, 6).unwrap());
        let make = || {
            generate_tracked_region_filmstrip(
                "tracked",
                &seq,
                tracked_params(
                    vec![
                        TrackedRegion {
                            frame_index: 0,
                            rect: rect(0, 0, 4, 3),
                        },
                        TrackedRegion {
                            frame_index: 2,
                            rect: rect(2, 1, 4, 3),
                        },
                    ],
                    3,
                ),
            )
            .unwrap()
        };
        assert_eq!(make().image().bytes(), make().image().bytes());
    }

    #[test]
    fn tracked_filmstrip_rejects_inconsistent_crop_dimensions() {
        let seq = sized_source(3, PixelDimensions::new(8, 6).unwrap());
        let result = generate_tracked_region_filmstrip(
            "tracked",
            &seq,
            tracked_params(
                vec![
                    TrackedRegion {
                        frame_index: 0,
                        rect: rect(0, 0, 4, 3),
                    },
                    TrackedRegion {
                        frame_index: 1,
                        rect: rect(0, 0, 2, 3),
                    },
                ],
                3,
            ),
        );
        assert!(result.is_err());
    }
}
