use crate::{ErrorCode, PixelDimensions, Result, VisionError, normalize::linear16_to_srgb8};

pub(crate) const BLACK: [u8; 3] = [10, 12, 16];
pub(crate) const PANEL: [u8; 3] = [26, 31, 39];
pub(crate) const WHITE: [u8; 3] = [244, 247, 250];
pub(crate) const MUTED: [u8; 3] = [174, 184, 198];
pub(crate) const WARNING: [u8; 3] = [255, 196, 64];

#[derive(Debug)]
pub(crate) struct Canvas {
    dimensions: PixelDimensions,
    pixels: Vec<u8>,
}

impl Canvas {
    pub(crate) fn new(
        dimensions: PixelDimensions,
        background: [u8; 3],
        max_bytes: usize,
    ) -> Result<Self> {
        let byte_len = dimensions
            .pixel_count()?
            .checked_mul(3)
            .ok_or_else(canvas_limit_error)?;
        if byte_len > max_bytes {
            return Err(canvas_limit_error());
        }
        let mut pixels = vec![0_u8; byte_len];
        for pixel in pixels.chunks_exact_mut(3) {
            pixel.copy_from_slice(&background);
        }
        Ok(Self { dimensions, pixels })
    }

    pub(crate) const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn fill_rect(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: [u8; 3],
    ) -> Result<()> {
        let right = x.checked_add(width).ok_or_else(canvas_limit_error)?;
        let bottom = y.checked_add(height).ok_or_else(canvas_limit_error)?;
        if right > self.dimensions.width() || bottom > self.dimensions.height() {
            return Err(canvas_limit_error());
        }
        for row in y..bottom {
            for column in x..right {
                self.set_pixel(column, row, color)?;
            }
        }
        Ok(())
    }

    pub(crate) fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 3]) -> Result<()> {
        if x >= self.dimensions.width() || y >= self.dimensions.height() {
            return Err(canvas_limit_error());
        }
        let index = usize::try_from(y)
            .ok()
            .and_then(|row| row.checked_mul(usize::try_from(self.dimensions.width()).ok()?))
            .and_then(|row| row.checked_add(usize::try_from(x).ok()?))
            .and_then(|pixel| pixel.checked_mul(3))
            .ok_or_else(canvas_limit_error)?;
        self.pixels[index..index + 3].copy_from_slice(&color);
        Ok(())
    }

    pub(crate) fn draw_hatch(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: [u8; 3],
    ) -> Result<()> {
        let right = x.checked_add(width).ok_or_else(canvas_limit_error)?;
        let bottom = y.checked_add(height).ok_or_else(canvas_limit_error)?;
        if right > self.dimensions.width() || bottom > self.dimensions.height() {
            return Err(canvas_limit_error());
        }
        for row in y..bottom {
            for column in x..right {
                if (column + row) % 6 < 2 {
                    self.set_pixel(column, row, color)?;
                }
            }
        }
        Ok(())
    }

    /// Contain-fit one normalized frame using integer nearest-neighbor center mapping.
    pub(crate) fn draw_linear_frame(
        &mut self,
        source_dimensions: PixelDimensions,
        linear_rgb16: &[u16],
        target_x: u32,
        target_y: u32,
        target_width: u32,
        target_height: u32,
    ) -> Result<(u32, u32, u32, u32)> {
        let expected = source_dimensions
            .pixel_count()?
            .checked_mul(3)
            .ok_or_else(canvas_limit_error)?;
        if linear_rgb16.len() != expected || target_width == 0 || target_height == 0 {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "source raster or target bounds are invalid",
            ));
        }
        let (draw_width, draw_height) = contain_fit(
            source_dimensions.width(),
            source_dimensions.height(),
            target_width,
            target_height,
        )?;
        let offset_x = target_x
            .checked_add((target_width - draw_width) / 2)
            .ok_or_else(canvas_limit_error)?;
        let offset_y = target_y
            .checked_add((target_height - draw_height) / 2)
            .ok_or_else(canvas_limit_error)?;
        for y in 0..draw_height {
            let source_y = center_map(y, source_dimensions.height(), draw_height)?;
            for x in 0..draw_width {
                let source_x = center_map(x, source_dimensions.width(), draw_width)?;
                let source_index = usize::try_from(source_y)
                    .ok()
                    .and_then(|row| {
                        row.checked_mul(usize::try_from(source_dimensions.width()).ok()?)
                    })
                    .and_then(|row| row.checked_add(usize::try_from(source_x).ok()?))
                    .and_then(|pixel| pixel.checked_mul(3))
                    .ok_or_else(canvas_limit_error)?;
                let color = [
                    linear16_to_srgb8(linear_rgb16[source_index]),
                    linear16_to_srgb8(linear_rgb16[source_index + 1]),
                    linear16_to_srgb8(linear_rgb16[source_index + 2]),
                ];
                self.set_pixel(offset_x + x, offset_y + y, color)?;
            }
        }
        Ok((offset_x, offset_y, draw_width, draw_height))
    }
}

fn contain_fit(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Result<(u32, u32)> {
    let width_limited_height = round_ratio(
        u128::from(target_width) * u128::from(source_height),
        u128::from(source_width),
    )?;
    if width_limited_height <= u128::from(target_height) {
        Ok((
            target_width,
            u32::try_from(width_limited_height.max(1)).map_err(|_| canvas_limit_error())?,
        ))
    } else {
        let height_limited_width = round_ratio(
            u128::from(target_height) * u128::from(source_width),
            u128::from(source_height),
        )?;
        Ok((
            u32::try_from(height_limited_width.max(1)).map_err(|_| canvas_limit_error())?,
            target_height,
        ))
    }
}

fn center_map(output: u32, source_len: u32, output_len: u32) -> Result<u32> {
    let numerator = u64::from(output)
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .and_then(|value| value.checked_mul(u64::from(source_len)))
        .ok_or_else(canvas_limit_error)?;
    let denominator = u64::from(output_len) * 2;
    Ok(u32::try_from(numerator / denominator)
        .map_err(|_| canvas_limit_error())?
        .min(source_len - 1))
}

fn round_ratio(numerator: u128, denominator: u128) -> Result<u128> {
    numerator
        .checked_add(denominator / 2)
        .map(|value| value / denominator)
        .ok_or_else(canvas_limit_error)
}

pub(crate) fn canvas_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "artifact raster exceeds configured layout or canvas limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_fit_and_center_mapping_are_checked_and_stable() {
        assert_eq!(contain_fit(4, 2, 10, 10).unwrap(), (10, 5));
        assert_eq!(contain_fit(2, 4, 10, 10).unwrap(), (5, 10));
        assert_eq!(center_map(0, 2, 4).unwrap(), 0);
        assert_eq!(center_map(3, 2, 4).unwrap(), 1);
        assert!(Canvas::new(PixelDimensions::new(2, 2).unwrap(), BLACK, 11).is_err());
    }
}
