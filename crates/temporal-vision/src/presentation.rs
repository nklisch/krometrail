use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ArtifactKind, ArtifactManifest, EncodedImage, ErrorCode, GeneratedArtifact, OutputHash,
    OwnedFrame, PixelDimensions, PixelRect, RenderLimits, Result, StoryboardSelection, VisionError,
    render::canvas::{Canvas, canvas_limit_error, original_fit_dimensions},
};

/// One selected original's image rectangle in the encoded montage, excluding padding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationTile<F> {
    frame_id: F,
    rect: PixelRect,
}

impl<F> PresentationTile<F> {
    pub fn frame_id(&self) -> &F {
        &self.frame_id
    }

    pub const fn rect(&self) -> PixelRect {
        self.rect
    }
}

/// Original-image presentation and the unchanged analysis selection behind it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentedStoryboard<F> {
    image: EncodedImage,
    selection: StoryboardSelection<F>,
    tiles: Vec<PresentationTile<F>>,
}

impl<F> PresentedStoryboard<F> {
    pub const fn image(&self) -> &EncodedImage {
        &self.image
    }

    pub const fn selection(&self) -> &StoryboardSelection<F> {
        &self.selection
    }

    pub fn tiles(&self) -> &[PresentationTile<F>] {
        &self.tiles
    }

    pub fn output_hash(&self) -> OutputHash {
        OutputHash::from_bytes(Sha256::digest(self.image.bytes()).into())
    }
}

impl<F: Clone + Eq> PresentedStoryboard<F> {
    /// Attach caller-supplied provenance for these exact bytes and selection.
    ///
    /// The caller retains responsibility for source/analysis populations and
    /// transformation parameters. Tile geometry is added and validated here.
    pub fn into_artifact<A, M: Clone + Eq, G: Clone + Eq>(
        self,
        manifest: ArtifactManifest<A, F, M, G>,
    ) -> Result<GeneratedArtifact<A, F, M, G>> {
        if manifest.artifact_kind() != ArtifactKind::Storyboard
            || manifest.output_dimensions() != self.image.dimensions()
            || manifest.output_hash() != self.output_hash()
            || manifest.storyboard_selection() != Some(&self.selection)
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest must describe the presented storyboard bytes and unchanged selection",
            ));
        }
        let manifest = manifest.with_presentation_tiles(self.tiles)?;
        Ok(GeneratedArtifact::new(self.image, manifest))
    }
}

/// Render selected originals without reselecting or normalizing analysis pixels.
///
/// Frames are loaded exactly once in selection order, fitted into small RGB8
/// copies, then dropped before the next load. The grid has up to three columns
/// in row-major order. Each column takes its widest fitted image's width and
/// each row its tallest image's height. Images start at their column/row offsets
/// with no centering padding, preserve aspect ratio (rounded to whole pixels),
/// and are never enlarged. Retained fitted pixels total at most the canvas size;
/// the canvas byte cap is checked before allocating fitted copies or the canvas.
/// Labels and annotations are left to the caller; rectangles name image pixels.
/// RGBA8 sRGB straight alpha is composited on black in encoded sRGB space.
///
/// Both image dimensions fit `max_edge`. The default [`RenderLimits`] canvas
/// and encoded-byte caps also apply. Loader failures are propagated unchanged.
pub fn render_storyboard_from_selection<F: Clone + Eq>(
    selection: &StoryboardSelection<F>,
    max_edge: u32,
    mut load: impl FnMut(&F) -> Result<OwnedFrame<F>>,
) -> Result<PresentedStoryboard<F>> {
    let count = selection.selected_frames().len();
    if count == 0 {
        return Err(VisionError::new(
            ErrorCode::EmptySequence,
            "storyboard selection is empty",
        ));
    }
    if max_edge == 0 {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "max_edge must be non-zero",
        ));
    }
    let columns = count.min(3);
    let rows = count.div_ceil(columns);
    let grid_edge = u32::try_from(columns.max(rows)).map_err(|_| canvas_limit_error())?;
    let tile_edge = max_edge / grid_edge;
    if tile_edge == 0 {
        return Err(canvas_limit_error());
    }
    let limits = RenderLimits::default();
    let mut widths = vec![0_u32; columns];
    let mut heights = vec![0_u32; rows];
    let mut fitted = Vec::with_capacity(count);
    for (index, selected) in selection.selected_frames().iter().enumerate() {
        let original = load(selected.frame_id())?;
        if original.id() != selected.frame_id() {
            return Err(VisionError::at(
                ErrorCode::IncompatibleFrame,
                "loaded frame id differs from the selected frame id",
                index,
            ));
        }
        let dimensions = original_fit_dimensions(original.dimensions(), tile_edge)?;
        widths[index % columns] = widths[index % columns].max(dimensions.width());
        heights[index / columns] = heights[index / columns].max(dimensions.height());
        // Extents only grow, so refuse before even allocating the next fitted
        // copy when the eventual canvas already exceeds the cap.
        packed_dimensions(&widths, &heights, limits.max_canvas_bytes())?;
        let mut image = Canvas::new(dimensions, [0, 0, 0], limits.max_canvas_bytes())?;
        image.draw_original(
            &original,
            PixelRect::new(0, 0, dimensions.width(), dimensions.height())?,
        )?;
        drop(original);
        fitted.push(image);
    }
    let dimensions = packed_dimensions(&widths, &heights, limits.max_canvas_bytes())?;
    // max_edge supplies the dimension ceiling; retain the renderer's byte caps.
    let mut canvas = Canvas::new(dimensions, [0, 0, 0], limits.max_canvas_bytes())?;
    let mut tiles = Vec::with_capacity(count);
    for (index, (selected, image)) in selection.selected_frames().iter().zip(fitted).enumerate() {
        let x = widths[..index % columns].iter().sum();
        let y = heights[..index / columns].iter().sum();
        let rect = canvas.copy_from(&image, x, y)?;
        tiles.push(PresentationTile {
            frame_id: selected.frame_id().clone(),
            rect,
        });
    }
    let (bytes, _) =
        crate::encode::encode_png(dimensions, canvas.pixels(), limits.max_encoded_bytes())?;
    Ok(PresentedStoryboard {
        image: EncodedImage::new(dimensions, bytes),
        selection: selection.clone(),
        tiles,
    })
}

fn packed_dimensions(widths: &[u32], heights: &[u32], max_bytes: usize) -> Result<PixelDimensions> {
    let sum = |lengths: &[u32]| {
        lengths.iter().try_fold(0_u32, |total, length| {
            total.checked_add(*length).ok_or_else(canvas_limit_error)
        })
    };
    let dimensions =
        PixelDimensions::new(sum(widths)?, sum(heights)?).map_err(|_| canvas_limit_error())?;
    let bytes = dimensions
        .pixel_count()?
        .checked_mul(3)
        .ok_or_else(canvas_limit_error)?;
    if bytes > max_bytes {
        return Err(canvas_limit_error());
    }
    Ok(dimensions)
}
