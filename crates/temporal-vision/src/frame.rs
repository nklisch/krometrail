use serde::{Deserialize, Deserializer, Serialize};

use crate::{ErrorCode, Result, VisionError};

/// Nanoseconds in one caller-declared sequence clock.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    pub const ZERO: Self = Self(0);

    pub const fn from_nanos(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

/// Non-zero source-frame pixel dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PixelDimensions {
    width: u32,
    height: u32,
}

impl PixelDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(VisionError::new(
                ErrorCode::InvalidDimensions,
                "pixel dimensions must be non-zero",
            ));
        }
        let dimensions = Self { width, height };
        dimensions.rgba8_byte_len()?;
        Ok(dimensions)
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub fn pixel_count(self) -> Result<usize> {
        let count = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidDimensions,
                    "pixel dimensions exceed the supported address space",
                )
            })?;
        usize::try_from(count).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidDimensions,
                "pixel dimensions exceed the supported address space",
            )
        })
    }

    pub fn rgba8_byte_len(self) -> Result<usize> {
        self.pixel_count()?.checked_mul(4).ok_or_else(|| {
            VisionError::new(
                ErrorCode::InvalidDimensions,
                "RGBA8 payload length exceeds the supported address space",
            )
        })
    }
}

impl<'de> Deserialize<'de> for PixelDimensions {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            width: u32,
            height: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.width, wire.height).map_err(serde::de::Error::custom)
    }
}

stable_registry! {
    /// Decoded pixel representation accepted by this crate.
    pub enum PixelFormat {
        Rgba8SrgbStraight => "rgba8_srgb_straight",
    }
}

/// A validated decoded frame using caller-owned identity and pixel storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Frame<Id, Pixels> {
    id: Id,
    timestamp: Timestamp,
    dimensions: PixelDimensions,
    pixel_format: PixelFormat,
    pixels: Pixels,
}

pub type OwnedFrame<Id> = Frame<Id, Box<[u8]>>;
pub type BorrowedFrame<'a, Id> = Frame<Id, &'a [u8]>;

impl<Id, Pixels: AsRef<[u8]>> Frame<Id, Pixels> {
    pub fn new(
        id: Id,
        timestamp: Timestamp,
        dimensions: PixelDimensions,
        pixel_format: PixelFormat,
        pixels: Pixels,
    ) -> Result<Self> {
        if pixels.as_ref().len() != dimensions.rgba8_byte_len()? {
            return Err(VisionError::new(
                ErrorCode::PixelLengthMismatch,
                "pixel payload length does not match frame dimensions and format",
            ));
        }
        Ok(Self {
            id,
            timestamp,
            dimensions,
            pixel_format,
            pixels,
        })
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub const fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_ref()
    }

    pub fn as_borrowed(&self) -> BorrowedFrame<'_, &Id> {
        // The source frame already passed the same payload validation.
        Frame {
            id: &self.id,
            timestamp: self.timestamp,
            dimensions: self.dimensions,
            pixel_format: self.pixel_format,
            pixels: self.pixels.as_ref(),
        }
    }

    pub fn into_parts(self) -> (Id, Timestamp, PixelDimensions, PixelFormat, Pixels) {
        (
            self.id,
            self.timestamp,
            self.dimensions,
            self.pixel_format,
            self.pixels,
        )
    }
}

impl<Id: Clone, Pixels: AsRef<[u8]>> Frame<Id, Pixels> {
    pub fn to_owned(&self) -> OwnedFrame<Id> {
        Frame {
            id: self.id.clone(),
            timestamp: self.timestamp,
            dimensions: self.dimensions,
            pixel_format: self.pixel_format,
            pixels: self.pixels.as_ref().into(),
        }
    }
}

impl<'de, Id, Pixels> Deserialize<'de> for Frame<Id, Pixels>
where
    Id: Deserialize<'de>,
    Pixels: Deserialize<'de> + AsRef<[u8]>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire<Id, Pixels> {
            id: Id,
            timestamp: Timestamp,
            dimensions: PixelDimensions,
            pixel_format: PixelFormat,
            pixels: Pixels,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.timestamp,
            wire.dimensions,
            wire.pixel_format,
            wire.pixels,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_frame_payload_and_deserialization() {
        let dimensions = PixelDimensions::new(2, 2).unwrap();
        assert_eq!(dimensions.rgba8_byte_len().unwrap(), 16);
        assert_eq!(
            Frame::new(
                "frame",
                Timestamp::ZERO,
                dimensions,
                PixelFormat::Rgba8SrgbStraight,
                &[0_u8; 15][..],
            )
            .unwrap_err()
            .code,
            ErrorCode::PixelLengthMismatch
        );
        assert!(serde_json::from_str::<PixelDimensions>(r#"{"width":0,"height":1}"#).is_err());
    }

    #[test]
    fn pixel_format_registry_is_stable() {
        for format in PixelFormat::ALL {
            let json = serde_json::to_string(format).unwrap();
            assert_eq!(serde_json::from_str::<PixelFormat>(&json).unwrap(), *format);
            assert_eq!(format.to_string(), format.as_str());
        }
    }
}
