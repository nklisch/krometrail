use crc32fast::Hasher;
use krometrail_core::{ByteOffset, EncodedFrame, FrameAddress};

use super::{
    FRAME_RECORD_KIND, FRAME_RECORD_PREFIX_LEN, SEGMENT_HEADER_LEN, SegmentHeader,
    decode_frame_record,
    wire::{WireReader, usize_from_u64},
};
use crate::persistence_error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordSpan {
    pub byte_offset: ByteOffset,
    pub encoded_len: u64,
    pub header_len: u32,
    pub payload_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trailing {
    Clean,
    Incomplete { at: ByteOffset },
    Corrupt { at: ByteOffset },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanResult {
    pub records: Vec<RecordSpan>,
    pub trailing: Trailing,
}

/// Scans a contiguous frame-record region.
///
/// The scanner interprets only each fixed length/checksum prefix. Metadata and
/// payload bytes remain opaque; they are touched solely to calculate CRC32.
pub fn scan_complete_records(bytes: &[u8]) -> ScanResult {
    let mut records = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let record_start = ByteOffset::new(offset as u64);
        if bytes.len() - offset < FRAME_RECORD_PREFIX_LEN {
            return ScanResult {
                records,
                trailing: Trailing::Incomplete { at: record_start },
            };
        }

        let mut prefix = WireReader::new(&bytes[offset..offset + FRAME_RECORD_PREFIX_LEN]);
        let Ok(kind) = prefix.read_u8() else {
            unreachable!("fixed prefix length was checked")
        };
        if kind != FRAME_RECORD_KIND {
            return ScanResult {
                records,
                trailing: Trailing::Corrupt { at: record_start },
            };
        }
        let header_len = prefix.read_u32().expect("fixed prefix length was checked");
        let payload_len = prefix.read_u64().expect("fixed prefix length was checked");
        let expected_crc = prefix.read_u32().expect("fixed prefix length was checked");
        let Ok(payload_len_usize) = usize_from_u64(payload_len) else {
            return ScanResult {
                records,
                trailing: Trailing::Corrupt { at: record_start },
            };
        };
        let Some(encoded_len) = FRAME_RECORD_PREFIX_LEN
            .checked_add(header_len as usize)
            .and_then(|value| value.checked_add(payload_len_usize))
        else {
            return ScanResult {
                records,
                trailing: Trailing::Corrupt { at: record_start },
            };
        };
        let Some(end) = offset.checked_add(encoded_len) else {
            return ScanResult {
                records,
                trailing: Trailing::Corrupt { at: record_start },
            };
        };
        if end > bytes.len() {
            return ScanResult {
                records,
                trailing: Trailing::Incomplete { at: record_start },
            };
        }

        let data_start = offset + FRAME_RECORD_PREFIX_LEN;
        let mut hasher = Hasher::new();
        hasher.update(&bytes[data_start..end]);
        if hasher.finalize() != expected_crc {
            return ScanResult {
                records,
                trailing: Trailing::Corrupt { at: record_start },
            };
        }
        records.push(RecordSpan {
            byte_offset: record_start,
            encoded_len: encoded_len as u64,
            header_len,
            payload_len,
        });
        offset = end;
    }
    ScanResult {
        records,
        trailing: Trailing::Clean,
    }
}

/// Reads one frame from a complete segment buffer using its absolute file address.
pub fn read_frame_at(
    segment_bytes: &[u8],
    address: FrameAddress,
) -> krometrail_core::Result<EncodedFrame> {
    let header_bytes = segment_bytes
        .get(..SEGMENT_HEADER_LEN)
        .ok_or_else(|| persistence_error("segment is shorter than its header"))?;
    let header = SegmentHeader::decode(header_bytes)?;
    if header.segment_id != address.segment_id {
        return Err(persistence_error(
            "frame address segment identifier does not match the segment header",
        ));
    }
    let offset = usize_from_u64(address.byte_offset.get())?;
    if offset < SEGMENT_HEADER_LEN {
        return Err(persistence_error(
            "frame address points inside the segment header",
        ));
    }
    let record_bytes = segment_bytes
        .get(offset..)
        .ok_or_else(|| persistence_error("frame address exceeds the segment length"))?;
    Ok(decode_frame_record(&header, record_bytes)?.frame)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::segments::{SegmentHeader, encode_frame_record};
    use krometrail_core::{
        CaptureOrdinal, CaptureWarning, CapturedFrame, DeviceScaleFactor, FrameId, ImageFormat,
        ObservedTime, PixelDimensions, SegmentId, SessionId, SessionTime, SourceTime, TargetId,
    };
    use uuid::Uuid;

    fn header() -> SegmentHeader {
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

    fn frame(format: ImageFormat, source_time: Option<SourceTime>) -> EncodedFrame {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(4)),
                header().session_id,
                header().target_id,
                CaptureOrdinal::new(1).unwrap(),
                source_time,
                ObservedTime::from_nanos(20),
                SessionTime::from_nanos(10),
                format,
                PixelDimensions::new(100, 80).unwrap(),
                PixelDimensions::new(90, 70).unwrap(),
                DeviceScaleFactor::new(1.25).unwrap(),
                vec![
                    CaptureWarning::MissingSourceTime,
                    CaptureWarning::ViewportMetadataIncomplete,
                ],
            )
            .unwrap(),
            Arc::<[u8]>::from([7, 8, 9]),
        )
        .unwrap()
    }

    #[test]
    fn scans_clean_incomplete_and_corrupt_records() {
        let first = encode_frame_record(&frame(ImageFormat::Jpeg, None)).unwrap();
        let second =
            encode_frame_record(&frame(ImageFormat::Png, Some(SourceTime::from_nanos(-7))))
                .unwrap();
        let mut clean = first.clone();
        clean.extend_from_slice(&second);
        let scan = scan_complete_records(&clean);
        assert_eq!(scan.trailing, Trailing::Clean);
        assert_eq!(scan.records.len(), 2);

        let truncated = &clean[..clean.len() - 1];
        let scan = scan_complete_records(truncated);
        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.trailing,
            Trailing::Incomplete {
                at: ByteOffset::new(first.len() as u64)
            }
        );

        let mut corrupt = clean;
        corrupt[first.len() + FRAME_RECORD_PREFIX_LEN + 1] ^= 1;
        let scan = scan_complete_records(&corrupt);
        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.trailing,
            Trailing::Corrupt {
                at: ByteOffset::new(first.len() as u64)
            }
        );
    }

    #[test]
    fn random_access_address_reconstructs_full_frame() {
        let expected = frame(ImageFormat::Png, Some(SourceTime::from_nanos(8)));
        let record = encode_frame_record(&expected).unwrap();
        let mut segment = header().encode();
        let address = FrameAddress::new(header().segment_id, ByteOffset::new(segment.len() as u64));
        segment.extend_from_slice(&record);
        assert_eq!(read_frame_at(&segment, address).unwrap(), expected);
    }
}
