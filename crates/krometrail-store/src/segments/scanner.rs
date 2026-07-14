use std::io::{Cursor, Read, Seek, SeekFrom};

use crc32fast::Hasher;
use krometrail_core::{ByteOffset, EncodedFrame, FrameAddress};

use super::{
    FRAME_RECORD_KIND, FRAME_RECORD_PREFIX_LEN, SEALED_FOOTER_LEN, SEALED_FOOTER_MAGIC,
    SEGMENT_HEADER_LEN, SealedFooter, SegmentHeader, decode_frame_record,
    wire::{WireReader, usize_from_u64},
};
use crate::persistence_error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordSpan {
    /// Absolute byte offset from the start of the segment file.
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

/// Scans all frame records in a complete segment buffer.
///
/// Returned offsets are absolute file offsets and can be copied directly into a
/// [`FrameAddress`]. The segment header is validated before any record lengths
/// are trusted. A valid sealed footer is recognized as the clean end of the
/// record region.
pub fn scan_complete_records(segment_bytes: &[u8]) -> krometrail_core::Result<ScanResult> {
    scan_complete_records_from(segment_bytes, ByteOffset::new(SEGMENT_HEADER_LEN as u64))
}

/// Scans a segment from a checked absolute offset.
///
/// This resume form exists for recovery callers. `base_offset` is always
/// interpreted relative to the start of `segment_bytes`; offsets inside the
/// header or beyond the supplied file are rejected rather than silently
/// producing relative addresses.
pub fn scan_complete_records_from(
    segment_bytes: &[u8],
    base_offset: ByteOffset,
) -> krometrail_core::Result<ScanResult> {
    let header_bytes = segment_bytes
        .get(..SEGMENT_HEADER_LEN)
        .ok_or_else(|| persistence_error("segment is shorter than its header"))?;
    let header = SegmentHeader::decode(header_bytes)?;
    let base = usize_from_u64(base_offset.get())?;
    if base < SEGMENT_HEADER_LEN {
        return Err(persistence_error(
            "segment scan base points inside the segment header",
        ));
    }
    if base > segment_bytes.len() {
        return Err(persistence_error(
            "segment scan base exceeds the segment length",
        ));
    }

    Ok(scan_record_region(segment_bytes, base, header))
}

fn scan_record_region(
    segment_bytes: &[u8],
    mut offset: usize,
    header: SegmentHeader,
) -> ScanResult {
    let mut records = Vec::new();
    while offset < segment_bytes.len() {
        let record_start = ByteOffset::new(offset as u64);
        let remaining = &segment_bytes[offset..];
        if looks_like_footer(remaining) {
            if remaining.len() != SEALED_FOOTER_LEN {
                return ScanResult {
                    records,
                    trailing: if remaining.len() < SEALED_FOOTER_LEN {
                        Trailing::Incomplete { at: record_start }
                    } else {
                        Trailing::Corrupt { at: record_start }
                    },
                };
            }
            let Ok(footer) = SealedFooter::decode(remaining) else {
                return ScanResult {
                    records,
                    trailing: Trailing::Corrupt { at: record_start },
                };
            };
            return ScanResult {
                records,
                trailing: if footer.segment_id == header.segment_id {
                    Trailing::Clean
                } else {
                    Trailing::Corrupt { at: record_start }
                },
            };
        }

        if remaining.len() < FRAME_RECORD_PREFIX_LEN {
            return ScanResult {
                records,
                trailing: Trailing::Incomplete { at: record_start },
            };
        }

        let mut prefix = WireReader::new(&remaining[..FRAME_RECORD_PREFIX_LEN]);
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
        if end > segment_bytes.len() {
            return ScanResult {
                records,
                trailing: Trailing::Incomplete { at: record_start },
            };
        }

        let data_start = offset + FRAME_RECORD_PREFIX_LEN;
        let mut hasher = Hasher::new();
        hasher.update(&segment_bytes[data_start..end]);
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

fn looks_like_footer(bytes: &[u8]) -> bool {
    let compared = bytes.len().min(SEALED_FOOTER_MAGIC.len());
    compared > 0 && bytes[..compared] == SEALED_FOOTER_MAGIC[..compared]
}

/// Reads only the addressed record from a seekable segment source.
pub fn read_frame_from<R: Read + Seek>(
    reader: &mut R,
    address: FrameAddress,
) -> krometrail_core::Result<EncodedFrame> {
    if address.byte_offset.get() < SEGMENT_HEADER_LEN as u64 {
        return Err(persistence_error(
            "frame address points inside the segment header",
        ));
    }
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|_| persistence_error("could not seek to the segment header"))?;
    let mut header_bytes = [0_u8; SEGMENT_HEADER_LEN];
    reader
        .read_exact(&mut header_bytes)
        .map_err(|_| persistence_error("could not read the segment header"))?;
    let header = SegmentHeader::decode(&header_bytes)?;
    if header.segment_id != address.segment_id {
        return Err(persistence_error(
            "frame address segment identifier does not match the segment header",
        ));
    }

    reader
        .seek(SeekFrom::Start(address.byte_offset.get()))
        .map_err(|_| persistence_error("could not seek to the frame record"))?;
    let mut prefix = [0_u8; FRAME_RECORD_PREFIX_LEN];
    reader
        .read_exact(&mut prefix)
        .map_err(|_| persistence_error("could not read the frame record prefix"))?;
    let mut fields = WireReader::new(&prefix);
    if fields.read_u8()? != FRAME_RECORD_KIND {
        return Err(persistence_error("segment record kind is not a frame"));
    }
    let header_len = fields.read_u32()? as usize;
    let payload_len = usize_from_u64(fields.read_u64()?)?;
    let tail_len = header_len
        .checked_add(payload_len)
        .ok_or_else(|| persistence_error("frame record length overflow"))?;
    let mut encoded = Vec::with_capacity(FRAME_RECORD_PREFIX_LEN + tail_len);
    encoded.extend_from_slice(&prefix);
    encoded.resize(FRAME_RECORD_PREFIX_LEN + tail_len, 0);
    reader
        .read_exact(&mut encoded[FRAME_RECORD_PREFIX_LEN..])
        .map_err(|_| persistence_error("could not read the complete frame record"))?;
    Ok(decode_frame_record(&header, &encoded)?.frame)
}

/// Reads one frame from a complete segment buffer using its absolute file address.
pub fn read_frame_at(
    segment_bytes: &[u8],
    address: FrameAddress,
) -> krometrail_core::Result<EncodedFrame> {
    read_frame_from(&mut Cursor::new(segment_bytes), address)
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

    fn open_segment(records: &[Vec<u8>]) -> Vec<u8> {
        let mut segment = header().encode();
        for record in records {
            segment.extend_from_slice(record);
        }
        segment
    }

    #[test]
    fn scans_clean_incomplete_and_corrupt_records_with_absolute_offsets() {
        let first = encode_frame_record(&frame(ImageFormat::Jpeg, None)).unwrap();
        let second =
            encode_frame_record(&frame(ImageFormat::Png, Some(SourceTime::from_nanos(-7))))
                .unwrap();
        let clean = open_segment(&[first.clone(), second.clone()]);
        let scan = scan_complete_records(&clean).unwrap();
        assert_eq!(scan.trailing, Trailing::Clean);
        assert_eq!(scan.records.len(), 2);
        assert_eq!(
            scan.records[0].byte_offset,
            ByteOffset::new(SEGMENT_HEADER_LEN as u64)
        );

        let truncated = &clean[..clean.len() - 1];
        let scan = scan_complete_records(truncated).unwrap();
        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.trailing,
            Trailing::Incomplete {
                at: ByteOffset::new((SEGMENT_HEADER_LEN + first.len()) as u64)
            }
        );

        let mut corrupt = clean;
        corrupt[SEGMENT_HEADER_LEN + first.len() + FRAME_RECORD_PREFIX_LEN + 1] ^= 1;
        let scan = scan_complete_records(&corrupt).unwrap();
        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.trailing,
            Trailing::Corrupt {
                at: ByteOffset::new((SEGMENT_HEADER_LEN + first.len()) as u64)
            }
        );
    }

    #[test]
    fn scanner_address_reads_from_a_full_sealed_segment() {
        let expected = frame(ImageFormat::Png, Some(SourceTime::from_nanos(8)));
        let record = encode_frame_record(&expected).unwrap();
        let mut segment = open_segment(&[record]);
        segment.extend_from_slice(
            &SealedFooter::new(
                header().segment_id,
                1,
                expected.byte_len().get(),
                expected.metadata().session_time(),
                expected.metadata().session_time(),
                expected.metadata().observed_time(),
            )
            .encode(),
        );

        let scan = scan_complete_records(&segment).unwrap();
        assert_eq!(scan.trailing, Trailing::Clean);
        let address = FrameAddress::new(header().segment_id, scan.records[0].byte_offset);
        assert_eq!(read_frame_at(&segment, address).unwrap(), expected);
    }

    #[test]
    fn scanner_rejects_invalid_headers_and_absolute_bases() {
        let segment = open_segment(&[]);
        assert!(scan_complete_records(&segment[..SEGMENT_HEADER_LEN - 1]).is_err());

        let mut corrupt_header = segment.clone();
        corrupt_header[10] ^= 1;
        assert!(scan_complete_records(&corrupt_header).is_err());

        assert!(
            scan_complete_records_from(&segment, ByteOffset::new(SEGMENT_HEADER_LEN as u64 - 1))
                .is_err()
        );
        assert!(
            scan_complete_records_from(&segment, ByteOffset::new(segment.len() as u64 + 1))
                .is_err()
        );
    }

    #[test]
    fn random_access_rejects_header_and_segment_mismatches() {
        let expected = frame(ImageFormat::Jpeg, None);
        let record = encode_frame_record(&expected).unwrap();
        let segment = open_segment(&[record]);
        assert!(
            read_frame_at(
                &segment,
                FrameAddress::new(header().segment_id, ByteOffset::new(1))
            )
            .is_err()
        );
        assert!(
            read_frame_at(
                &segment,
                FrameAddress::new(
                    SegmentId::from_uuid(Uuid::from_u128(99)),
                    ByteOffset::new(SEGMENT_HEADER_LEN as u64)
                )
            )
            .is_err()
        );
    }
}
