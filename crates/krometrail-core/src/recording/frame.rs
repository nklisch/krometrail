use std::{
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{FrameId, SessionId, TargetId},
    time::{ObservedTime, SessionTime, SourceTime},
    validation::deserialize_validated,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Jpeg,
    Png,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PixelDimensions {
    width: NonZeroU32,
    height: NonZeroU32,
}

#[derive(Deserialize)]
struct PixelDimensionsWire {
    width: u32,
    height: u32,
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

impl<'de> Deserialize<'de> for PixelDimensions {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: PixelDimensionsWire| {
            Self::new(wire.width, wire.height)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
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

impl<'de> Deserialize<'de> for DeviceScaleFactor {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: f64| Self::new(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CaptureOrdinal(NonZeroU64);

impl CaptureOrdinal {
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| invalid("capture ordinal must be non-zero"))
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for CaptureOrdinal {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: u64| Self::new(value))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureWarning {
    MissingSourceTime,
    SourceTimestampRounded,
    ViewportMetadataIncomplete,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CapturedFrame {
    id: FrameId,
    session_id: SessionId,
    target_id: TargetId,
    capture_ordinal: CaptureOrdinal,
    source_time: Option<SourceTime>,
    observed_time: ObservedTime,
    session_time: SessionTime,
    format: ImageFormat,
    image: PixelDimensions,
    viewport: PixelDimensions,
    device_scale_factor: DeviceScaleFactor,
    warnings: Vec<CaptureWarning>,
}

#[derive(Deserialize)]
struct CapturedFrameWire {
    id: FrameId,
    session_id: SessionId,
    target_id: TargetId,
    capture_ordinal: CaptureOrdinal,
    source_time: Option<SourceTime>,
    observed_time: ObservedTime,
    session_time: SessionTime,
    format: ImageFormat,
    image: PixelDimensions,
    viewport: PixelDimensions,
    device_scale_factor: DeviceScaleFactor,
    warnings: Vec<CaptureWarning>,
}

impl CapturedFrame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: FrameId,
        session_id: SessionId,
        target_id: TargetId,
        capture_ordinal: CaptureOrdinal,
        source_time: Option<SourceTime>,
        observed_time: ObservedTime,
        session_time: SessionTime,
        format: ImageFormat,
        image: PixelDimensions,
        viewport: PixelDimensions,
        device_scale_factor: DeviceScaleFactor,
        warnings: Vec<CaptureWarning>,
    ) -> Result<Self> {
        let frame = Self {
            id,
            session_id,
            target_id,
            capture_ordinal,
            source_time,
            observed_time,
            session_time,
            format,
            image,
            viewport,
            device_scale_factor,
            warnings,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub const fn id(&self) -> FrameId {
        self.id
    }
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub const fn target_id(&self) -> TargetId {
        self.target_id
    }
    pub const fn capture_ordinal(&self) -> CaptureOrdinal {
        self.capture_ordinal
    }
    pub const fn source_time(&self) -> Option<SourceTime> {
        self.source_time
    }
    pub const fn observed_time(&self) -> ObservedTime {
        self.observed_time
    }
    pub const fn session_time(&self) -> SessionTime {
        self.session_time
    }
    pub const fn format(&self) -> ImageFormat {
        self.format
    }
    pub const fn image(&self) -> PixelDimensions {
        self.image
    }
    pub const fn viewport(&self) -> PixelDimensions {
        self.viewport
    }
    pub const fn device_scale_factor(&self) -> DeviceScaleFactor {
        self.device_scale_factor
    }
    pub fn warnings(&self) -> &[CaptureWarning] {
        &self.warnings
    }

    /// A normalized frame timestamp cannot be later than the observed timestamp.
    /// The source clock remains independent and is intentionally not compared here.
    pub fn validate(&self) -> Result<()> {
        if self.session_time.as_nanos() > self.observed_time.as_nanos() {
            return Err(invalid("frame session time must not exceed observed time"));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for CapturedFrame {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: CapturedFrameWire| {
            Self::new(
                wire.id,
                wire.session_id,
                wire.target_id,
                wire.capture_ordinal,
                wire.source_time,
                wire.observed_time,
                wire.session_time,
                wire.format,
                wire.image,
                wire.viewport,
                wire.device_scale_factor,
                wire.warnings,
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncodedFrame {
    metadata: CapturedFrame,
    bytes: Arc<[u8]>,
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

    pub fn metadata(&self) -> &CapturedFrame {
        &self.metadata
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Clones the owned request-safe payload reference without copying bytes.
    pub fn encoded_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
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
        CapturedFrame::new(
            FrameId::from_uuid(UUID.parse().unwrap()),
            SessionId::from_uuid(UUID.parse().unwrap()),
            TargetId::from_uuid(UUID.parse().unwrap()),
            CaptureOrdinal::new(1).unwrap(),
            Some(SourceTime::from_nanos(20)),
            ObservedTime::from_nanos(30),
            SessionTime::from_nanos(10),
            ImageFormat::Jpeg,
            PixelDimensions::new(100, 80).unwrap(),
            PixelDimensions::new(100, 80).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn capture_ordinals_are_non_zero_and_round_trip_as_transparent_wire_values() {
        assert!(CaptureOrdinal::new(0).is_err());
        assert_eq!(CaptureOrdinal::new(7).unwrap().get(), 7);
        assert_eq!(
            serde_json::to_string(&CaptureOrdinal::new(7).unwrap()).unwrap(),
            "7"
        );
        assert_eq!(
            serde_json::from_str::<CaptureOrdinal>("7").unwrap(),
            CaptureOrdinal::new(7).unwrap()
        );
        assert!(serde_json::from_str::<CaptureOrdinal>("0").is_err());
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
    fn rejects_incoherent_frame_times() {
        assert!(
            CapturedFrame::new(
                FrameId::from_uuid(UUID.parse().unwrap()),
                SessionId::from_uuid(UUID.parse().unwrap()),
                TargetId::from_uuid(UUID.parse().unwrap()),
                CaptureOrdinal::new(1).unwrap(),
                None,
                ObservedTime::from_nanos(2),
                SessionTime::from_nanos(3),
                ImageFormat::Jpeg,
                PixelDimensions::new(1, 1).unwrap(),
                PixelDimensions::new(1, 1).unwrap(),
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![]
            )
            .is_err()
        );
    }

    #[test]
    fn preserves_three_time_values_independently() {
        let frame = metadata();
        assert_eq!(frame.source_time().unwrap().as_nanos(), 20);
        assert_eq!(frame.observed_time().as_nanos(), 30);
        assert_eq!(frame.session_time().as_nanos(), 10);
    }

    #[test]
    fn rejects_malformed_serialized_dimensions_scale_and_frames() {
        assert!(serde_json::from_str::<PixelDimensions>(r#"{"width":0,"height":80}"#).is_err());
        assert!(serde_json::from_str::<DeviceScaleFactor>("0.0").is_err());
        let malformed = serde_json::to_value(metadata()).unwrap();
        let mut object = malformed.as_object().unwrap().clone();
        object.insert("session_time".into(), serde_json::json!(31));
        assert!(serde_json::from_value::<CapturedFrame>(object.into()).is_err());
        let valid = metadata();
        let encoded = serde_json::to_string(&valid).unwrap();
        assert_eq!(
            serde_json::from_str::<CapturedFrame>(&encoded).unwrap(),
            valid
        );
    }
}
