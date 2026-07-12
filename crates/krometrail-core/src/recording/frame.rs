use std::{
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{FrameId, SessionId, TargetId},
    time::{ObservedTime, SessionTime, SourceTime},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Jpeg,
    Png,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixelDimensions {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl PixelDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        match (NonZeroU32::new(width), NonZeroU32::new(height)) {
            (Some(width), Some(height)) => Ok(Self { width, height }),
            _ => Err(invalid(
                "pixel dimensions must have non-zero width and height",
            )),
        }
    }

    pub const fn width(self) -> u32 {
        self.width.get()
    }

    pub const fn height(self) -> u32 {
        self.height.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceScaleFactor(f64);

impl DeviceScaleFactor {
    pub fn new(value: f64) -> Result<Self> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value))
        } else {
            Err(invalid("device scale factor must be finite and positive"))
        }
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureWarning {
    MissingSourceTime,
    SourceTimestampRounded,
    SourceSequenceDiscontinuity,
    ViewportMetadataIncomplete,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapturedFrame {
    pub id: FrameId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub source_sequence: u64,
    pub source_time: Option<SourceTime>,
    pub observed_time: ObservedTime,
    pub session_time: SessionTime,
    pub format: ImageFormat,
    pub image: PixelDimensions,
    pub viewport: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
    pub warnings: Vec<CaptureWarning>,
}

impl CapturedFrame {
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncodedFrame {
    pub metadata: CapturedFrame,
    pub bytes: Arc<[u8]>,
}

impl EncodedFrame {
    pub fn new(metadata: CapturedFrame, bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        metadata.validate()?;
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(invalid("encoded frame payload must not be empty"));
        }
        Ok(Self { metadata, bytes })
    }

    pub fn byte_len(&self) -> NonZeroU64 {
        NonZeroU64::new(self.bytes.len() as u64).expect("validated frame payload is non-empty")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn metadata() -> CapturedFrame {
        CapturedFrame {
            id: FrameId::from_uuid(UUID.parse().unwrap()),
            session_id: SessionId::from_uuid(UUID.parse().unwrap()),
            target_id: TargetId::from_uuid(UUID.parse().unwrap()),
            source_sequence: 1,
            source_time: Some(SourceTime::from_nanos(20)),
            observed_time: ObservedTime::from_nanos(30),
            session_time: SessionTime::from_nanos(10),
            format: ImageFormat::Jpeg,
            image: PixelDimensions::new(100, 80).unwrap(),
            viewport: PixelDimensions::new(100, 80).unwrap(),
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
            warnings: vec![],
        }
    }

    #[test]
    fn validates_dimensions_scale_and_payload() {
        assert!(PixelDimensions::new(0, 80).is_err());
        assert!(PixelDimensions::new(100, 0).is_err());
        assert!(DeviceScaleFactor::new(f64::NAN).is_err());
        assert!(DeviceScaleFactor::new(0.0).is_err());
        assert!(EncodedFrame::new(metadata(), Vec::<u8>::new()).is_err());
        assert_eq!(
            EncodedFrame::new(metadata(), vec![1, 2])
                .unwrap()
                .byte_len()
                .get(),
            2
        );
    }

    #[test]
    fn preserves_three_time_values_independently() {
        let frame = metadata();
        assert_eq!(frame.source_time.unwrap().as_nanos(), 20);
        assert_eq!(frame.observed_time.as_nanos(), 30);
        assert_eq!(frame.session_time.as_nanos(), 10);
    }
}
