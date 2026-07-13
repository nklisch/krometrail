use crc32fast::Hasher;
use krometrail_core::{
    CaptureOrdinal, CaptureWarning, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId,
    ImageFormat, ObservedTime, PixelDimensions, SessionTime, SourceTime,
};

use super::{SegmentHeader, wire::WireReader};
use crate::persistence_error;

pub const FRAME_RECORD_KIND: u8 = 0x01;
pub const FRAME_RECORD_PREFIX_LEN: usize = 17;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameRecord {
    pub header_bytes: Vec<u8>,
    pub payload_bytes: Vec<u8>,
}

impl FrameRecord {
    pub fn from_frame(frame: &EncodedFrame) -> krometrail_core::Result<Self> {
        let metadata = frame.metadata();
        let warning_count = u16::try_from(metadata.warnings().len())
            .map_err(|_| persistence_error("frame has too many capture warnings"))?;
        let mut header = Vec::with_capacity(84 + metadata.warnings().len());
        header.extend_from_slice(metadata.id().as_uuid().as_bytes());
        header.extend_from_slice(&metadata.capture_ordinal().get().to_be_bytes());
        match metadata.source_time() {
            Some(source_time) => {
                header.push(1);
                header.extend_from_slice(&source_time.as_nanos().to_be_bytes());
            }
            None => header.push(0),
        }
        header.extend_from_slice(&metadata.observed_time().as_nanos().to_be_bytes());
        header.extend_from_slice(&metadata.session_time().as_nanos().to_be_bytes());
        header.push(format_code(metadata.format()));
        header.extend_from_slice(&metadata.image().width().to_be_bytes());
        header.extend_from_slice(&metadata.image().height().to_be_bytes());
        header.extend_from_slice(&metadata.viewport().width().to_be_bytes());
        header.extend_from_slice(&metadata.viewport().height().to_be_bytes());
        header.extend_from_slice(&metadata.device_scale_factor().get().to_bits().to_be_bytes());
        header.extend_from_slice(&warning_count.to_be_bytes());
        for warning in metadata.warnings() {
            header.push(warning_code(warning));
        }
        Ok(Self {
            header_bytes: header,
            payload_bytes: frame.bytes().to_vec(),
        })
    }

    pub fn encode(&self) -> krometrail_core::Result<Vec<u8>> {
        let header_len = u32::try_from(self.header_bytes.len())
            .map_err(|_| persistence_error("frame metadata header is too large"))?;
        let payload_len = u64::try_from(self.payload_bytes.len())
            .map_err(|_| persistence_error("frame payload is too large"))?;
        let mut hasher = Hasher::new();
        hasher.update(&self.header_bytes);
        hasher.update(&self.payload_bytes);

        let mut bytes = Vec::with_capacity(
            FRAME_RECORD_PREFIX_LEN + self.header_bytes.len() + self.payload_bytes.len(),
        );
        bytes.push(FRAME_RECORD_KIND);
        bytes.extend_from_slice(&header_len.to_be_bytes());
        bytes.extend_from_slice(&payload_len.to_be_bytes());
        bytes.extend_from_slice(&hasher.finalize().to_be_bytes());
        bytes.extend_from_slice(&self.header_bytes);
        bytes.extend_from_slice(&self.payload_bytes);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedFrameRecord {
    pub frame: EncodedFrame,
    pub encoded_len: usize,
}

pub fn encode_frame_record(frame: &EncodedFrame) -> krometrail_core::Result<Vec<u8>> {
    FrameRecord::from_frame(frame)?.encode()
}

pub fn decode_frame_record(
    segment_header: &SegmentHeader,
    bytes: &[u8],
) -> krometrail_core::Result<DecodedFrameRecord> {
    let mut prefix = WireReader::new(bytes);
    if prefix.read_u8()? != FRAME_RECORD_KIND {
        return Err(persistence_error("segment record kind is not a frame"));
    }
    let header_len = usize::try_from(prefix.read_u32()?)
        .map_err(|_| persistence_error("frame metadata length exceeds this platform"))?;
    let payload_len = super::wire::usize_from_u64(prefix.read_u64()?)?;
    let expected_crc = prefix.read_u32()?;
    let encoded_len = FRAME_RECORD_PREFIX_LEN
        .checked_add(header_len)
        .and_then(|value| value.checked_add(payload_len))
        .ok_or_else(|| persistence_error("frame record length overflow"))?;
    if bytes.len() < encoded_len {
        return Err(persistence_error(
            "frame record ended before its declared length",
        ));
    }

    let header_bytes = &bytes[FRAME_RECORD_PREFIX_LEN..FRAME_RECORD_PREFIX_LEN + header_len];
    let payload_bytes = &bytes[FRAME_RECORD_PREFIX_LEN + header_len..encoded_len];
    let mut hasher = Hasher::new();
    hasher.update(header_bytes);
    hasher.update(payload_bytes);
    if hasher.finalize() != expected_crc {
        return Err(persistence_error("frame record CRC32 mismatch"));
    }

    let metadata = decode_metadata(segment_header, header_bytes)?;
    let frame = EncodedFrame::new(metadata, payload_bytes.to_vec())
        .map_err(|error| persistence_error(format!("invalid stored frame: {}", error.message)))?;
    Ok(DecodedFrameRecord { frame, encoded_len })
}

fn decode_metadata(
    segment_header: &SegmentHeader,
    bytes: &[u8],
) -> krometrail_core::Result<CapturedFrame> {
    let mut reader = WireReader::new(bytes);
    let frame_id = FrameId::from_uuid(reader.read_uuid()?);
    let capture_ordinal = CaptureOrdinal::new(reader.read_u64()?)
        .map_err(|error| persistence_error(format!("invalid stored frame: {}", error.message)))?;
    let source_time = match reader.read_u8()? {
        0 => None,
        1 => Some(SourceTime::from_nanos(reader.read_i128()?)),
        value => {
            return Err(persistence_error(format!(
                "invalid source-time presence code {value}"
            )));
        }
    };
    let observed_time = ObservedTime::from_nanos(reader.read_u64()?);
    let session_time = SessionTime::from_nanos(reader.read_u64()?);
    let format = decode_format(reader.read_u8()?)?;
    let image = dimensions(reader.read_u32()?, reader.read_u32()?)?;
    let viewport = dimensions(reader.read_u32()?, reader.read_u32()?)?;
    let device_scale_factor = DeviceScaleFactor::new(reader.read_f64()?)
        .map_err(|error| persistence_error(format!("invalid stored frame: {}", error.message)))?;
    let warning_count = usize::from(reader.read_u16()?);
    let mut warnings = Vec::with_capacity(warning_count);
    for _ in 0..warning_count {
        warnings.push(decode_warning(reader.read_u8()?)?);
    }
    if reader.remaining() != 0 {
        return Err(persistence_error(
            "frame metadata contains undeclared trailing bytes",
        ));
    }

    CapturedFrame::new(
        frame_id,
        segment_header.session_id,
        segment_header.target_id,
        capture_ordinal,
        source_time,
        observed_time,
        session_time,
        format,
        image,
        viewport,
        device_scale_factor,
        warnings,
    )
    .map_err(|error| persistence_error(format!("invalid stored frame: {}", error.message)))
}

fn dimensions(width: u32, height: u32) -> krometrail_core::Result<PixelDimensions> {
    PixelDimensions::new(width, height)
        .map_err(|error| persistence_error(format!("invalid stored frame: {}", error.message)))
}

const fn format_code(format: ImageFormat) -> u8 {
    match format {
        ImageFormat::Jpeg => 0,
        ImageFormat::Png => 1,
    }
}

fn decode_format(code: u8) -> krometrail_core::Result<ImageFormat> {
    match code {
        0 => Ok(ImageFormat::Jpeg),
        1 => Ok(ImageFormat::Png),
        value => Err(persistence_error(format!(
            "invalid stored image format code {value}"
        ))),
    }
}

const fn warning_code(warning: &CaptureWarning) -> u8 {
    match warning {
        CaptureWarning::MissingSourceTime => 0,
        CaptureWarning::SourceTimestampRounded => 1,
        CaptureWarning::ViewportMetadataIncomplete => 2,
    }
}

fn decode_warning(code: u8) -> krometrail_core::Result<CaptureWarning> {
    match code {
        0 => Ok(CaptureWarning::MissingSourceTime),
        1 => Ok(CaptureWarning::SourceTimestampRounded),
        2 => Ok(CaptureWarning::ViewportMetadataIncomplete),
        value => Err(persistence_error(format!(
            "invalid capture-warning code {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{SegmentId, SessionId, TargetId};
    use std::sync::Arc;
    use uuid::Uuid;

    fn segment_header() -> SegmentHeader {
        SegmentHeader::new(
            SegmentId::from_uuid(Uuid::from_u128(1)),
            SessionId::from_uuid(Uuid::from_u128(2)),
            TargetId::from_uuid(Uuid::from_u128(3)),
            SessionTime::ZERO,
            ObservedTime::from_nanos(1),
            2,
            3,
        )
    }

    fn frame(
        format: ImageFormat,
        source_time: Option<SourceTime>,
        scale: f64,
        warnings: Vec<CaptureWarning>,
    ) -> EncodedFrame {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(4)),
                segment_header().session_id,
                segment_header().target_id,
                CaptureOrdinal::new(5).unwrap(),
                source_time,
                ObservedTime::from_nanos(7),
                SessionTime::from_nanos(6),
                format,
                PixelDimensions::new(8, 9).unwrap(),
                PixelDimensions::new(10, 11).unwrap(),
                DeviceScaleFactor::new(scale).unwrap(),
                warnings,
            )
            .unwrap(),
            Arc::<[u8]>::from([12, 13, 14]),
        )
        .unwrap()
    }

    #[test]
    fn frame_record_round_trips_the_full_metadata_surface() {
        let cases = [
            frame(ImageFormat::Jpeg, None, 1.0, vec![]),
            frame(
                ImageFormat::Png,
                Some(SourceTime::from_nanos(-10)),
                2.0,
                vec![CaptureWarning::SourceTimestampRounded],
            ),
            frame(
                ImageFormat::Jpeg,
                Some(SourceTime::from_nanos(10)),
                1.25,
                vec![
                    CaptureWarning::MissingSourceTime,
                    CaptureWarning::SourceTimestampRounded,
                    CaptureWarning::ViewportMetadataIncomplete,
                ],
            ),
        ];
        for expected in cases {
            let bytes = encode_frame_record(&expected).unwrap();
            let decoded = decode_frame_record(&segment_header(), &bytes).unwrap();
            assert_eq!(decoded.frame, expected);
            assert_eq!(decoded.encoded_len, bytes.len());
        }
    }

    #[test]
    fn frame_record_is_big_endian_and_crc_covers_metadata_and_payload() {
        let expected = frame(ImageFormat::Jpeg, None, 1.0, vec![]);
        let encoded = encode_frame_record(&expected).unwrap();
        assert_eq!(&encoded[..13], &[1, 0, 0, 0, 68, 0, 0, 0, 0, 0, 0, 0, 3]);

        let mut corrupt_header = encoded.clone();
        corrupt_header[FRAME_RECORD_PREFIX_LEN + 1] ^= 1;
        assert!(
            decode_frame_record(&segment_header(), &corrupt_header)
                .unwrap_err()
                .message
                .as_str()
                .contains("CRC32")
        );

        let mut corrupt_payload = encoded;
        *corrupt_payload.last_mut().unwrap() ^= 1;
        assert!(
            decode_frame_record(&segment_header(), &corrupt_payload)
                .unwrap_err()
                .message
                .as_str()
                .contains("CRC32")
        );
    }
}
