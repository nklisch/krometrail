use std::num::{NonZeroU8, NonZeroU32};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{ErrorCode, FrameSequence, PixelDimensions, PixelRect, Result, Timestamp, VisionError};

stable_registry! {
    /// Coordinate space in which a fixed filmstrip region was declared.
    pub enum RegionCoordinateSpace {
        SourceImage => "source_image",
        Viewport => "viewport",
    }
}

/// A non-empty half-open rectangle whose origin may lie outside the source image.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

    let omitted_frame_count = u64::try_from(source.frames().len() - tiles.len()).map_err(|_| {
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

fn select_indices(frame_count: usize, limit: usize) -> Vec<usize> {
    if frame_count <= limit {
        return (0..frame_count).collect();
    }
    if limit == 1 {
        return vec![0];
    }
    let span = frame_count - 1;
    let denominator = limit - 1;
    (0..limit)
        .map(|slot| round_ratio_ties_down(slot * span, denominator))
        .collect()
}

fn round_ratio_ties_down(numerator: usize, denominator: usize) -> usize {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    quotient + usize::from(remainder > denominator - remainder)
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
}
