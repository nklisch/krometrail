use serde::{Deserialize, Deserializer, Serialize};

use crate::{ErrorCode, PixelDimensions, Result, VisionError};

/// A non-empty half-open rectangle in source-frame pixel coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "pixel rectangle dimensions must be non-zero",
            ));
        }
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

    pub const fn x(self) -> u32 {
        self.x
    }

    pub const fn y(self) -> u32 {
        self.y
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub fn right_exclusive(self) -> Result<u32> {
        self.x.checked_add(self.width).ok_or_else(|| {
            VisionError::new(
                ErrorCode::InvalidRegion,
                "pixel rectangle exceeds the coordinate space",
            )
        })
    }

    pub fn bottom_exclusive(self) -> Result<u32> {
        self.y.checked_add(self.height).ok_or_else(|| {
            VisionError::new(
                ErrorCode::InvalidRegion,
                "pixel rectangle exceeds the coordinate space",
            )
        })
    }

    pub fn fits_within(self, dimensions: PixelDimensions) -> bool {
        self.right_exclusive()
            .is_ok_and(|right| right <= dimensions.width())
            && self
                .bottom_exclusive()
                .is_ok_and(|bottom| bottom <= dimensions.height())
    }
}

impl<'de> Deserialize<'de> for PixelRect {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            x: u32,
            y: u32,
            width: u32,
            height: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.x, wire.y, wire.width, wire.height).map_err(serde::de::Error::custom)
    }
}

/// A rectangle validated against one source-frame geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct FrameRegion {
    rect: PixelRect,
}

impl FrameRegion {
    pub fn new(rect: PixelRect, frame_dimensions: PixelDimensions) -> Result<Self> {
        if !rect.fits_within(frame_dimensions) {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "frame region lies outside the source-frame dimensions",
            ));
        }
        Ok(Self { rect })
    }

    pub const fn rect(self) -> PixelRect {
        self.rect
    }
}

impl<'de> Deserialize<'de> for FrameRegion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            rect: PixelRect,
        }
        let wire = Wire::deserialize(deserializer)?;
        // PixelRect's validated deserializer preserves every invariant retained by
        // FrameRegion. Containment is checked again against sequence dimensions.
        Ok(Self { rect: wire.rect })
    }
}

/// A full-frame row-major, MSB-first, one-bit-per-pixel mask.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BinaryMask {
    dimensions: PixelDimensions,
    bits: Box<[u8]>,
}

impl BinaryMask {
    pub fn new(dimensions: PixelDimensions, bits: impl Into<Box<[u8]>>) -> Result<Self> {
        let bits = bits.into();
        let pixel_count = dimensions.pixel_count()?;
        let expected_len = pixel_count.checked_add(7).ok_or_else(|| {
            VisionError::new(ErrorCode::InvalidMask, "mask dimensions are too large")
        })? / 8;
        if bits.len() != expected_len {
            return Err(VisionError::new(
                ErrorCode::InvalidMask,
                "mask byte length does not match its dimensions",
            ));
        }
        let padding = expected_len * 8 - pixel_count;
        if padding > 0 {
            let padding_mask = (1_u8 << padding) - 1;
            if bits.last().is_some_and(|byte| byte & padding_mask != 0) {
                return Err(VisionError::new(
                    ErrorCode::InvalidMask,
                    "unused trailing mask bits must be zero",
                ));
            }
        }
        Ok(Self { dimensions, bits })
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    pub fn includes(&self, x: u32, y: u32) -> Option<bool> {
        if x >= self.dimensions.width() || y >= self.dimensions.height() {
            return None;
        }
        let index = usize::try_from(y).ok()? * usize::try_from(self.dimensions.width()).ok()?
            + usize::try_from(x).ok()?;
        let byte = self.bits[index / 8];
        Some(byte & (0x80 >> (index % 8)) != 0)
    }

    /// Returns the smallest source-frame rectangle containing every selected bit.
    ///
    /// The mask always remains full-frame; this rectangle is only a crop plan and
    /// does not change the mask coordinate space.
    pub fn bounds(&self) -> Result<Option<PixelRect>> {
        let mut left = self.dimensions.width();
        let mut top = self.dimensions.height();
        let mut right = 0_u32;
        let mut bottom = 0_u32;
        let mut selected = false;
        for y in 0..self.dimensions.height() {
            for x in 0..self.dimensions.width() {
                if self.includes(x, y) == Some(true) {
                    selected = true;
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x.checked_add(1).ok_or_else(|| {
                        VisionError::new(ErrorCode::InvalidMask, "mask bounds overflow")
                    })?);
                    bottom = bottom.max(y.checked_add(1).ok_or_else(|| {
                        VisionError::new(ErrorCode::InvalidMask, "mask bounds overflow")
                    })?);
                }
            }
        }
        if !selected {
            return Ok(None);
        }
        Ok(Some(PixelRect::new(left, top, right - left, bottom - top)?))
    }
}

impl<'de> Deserialize<'de> for BinaryMask {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            dimensions: PixelDimensions,
            bits: Box<[u8]>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.dimensions, wire.bits).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_checked_geometry_and_mask_padding() {
        let dimensions = PixelDimensions::new(3, 3).unwrap();
        assert!(PixelRect::new(u32::MAX, 0, 1, 1).is_err());
        assert!(FrameRegion::new(PixelRect::new(2, 2, 2, 1).unwrap(), dimensions).is_err());
        assert_eq!(
            BinaryMask::new(dimensions, [0xff, 0x01]).unwrap_err().code,
            ErrorCode::InvalidMask
        );
        let mask = BinaryMask::new(dimensions, [0x80, 0x00]).unwrap();
        assert_eq!(mask.includes(0, 0), Some(true));
        assert_eq!(mask.includes(1, 0), Some(false));
        assert_eq!(mask.includes(3, 0), None);
        assert_eq!(
            mask.bounds().unwrap(),
            Some(PixelRect::new(0, 0, 1, 1).unwrap())
        );
        assert_eq!(
            BinaryMask::new(dimensions, [0x00, 0x00])
                .unwrap()
                .bounds()
                .unwrap(),
            None
        );
    }
}
