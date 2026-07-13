use krometrail_core::{ObservedTime, SegmentId, SessionId, SessionTime, TargetId};

use super::wire::{WireReader, put_uuid};
use crate::persistence_error;

pub const SEGMENT_MAGIC: &[u8; 4] = b"KTSF";
pub const FORMAT_VERSION: u16 = 1;
pub const SEGMENT_HEADER_LEN: usize = 90;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentHeader {
    pub segment_id: SegmentId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub start_session_time: SessionTime,
    pub created_observed: ObservedTime,
    pub rotation_max_duration_nanos: u64,
    pub rotation_max_size: u64,
}

impl SegmentHeader {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        segment_id: SegmentId,
        session_id: SessionId,
        target_id: TargetId,
        start_session_time: SessionTime,
        created_observed: ObservedTime,
        rotation_max_duration_nanos: u64,
        rotation_max_size: u64,
    ) -> Self {
        Self {
            segment_id,
            session_id,
            target_id,
            start_session_time,
            created_observed,
            rotation_max_duration_nanos,
            rotation_max_size,
        }
    }

    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SEGMENT_HEADER_LEN);
        bytes.extend_from_slice(SEGMENT_MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
        put_uuid(&mut bytes, self.segment_id.as_uuid());
        put_uuid(&mut bytes, self.session_id.as_uuid());
        put_uuid(&mut bytes, self.target_id.as_uuid());
        bytes.extend_from_slice(&self.start_session_time.as_nanos().to_be_bytes());
        bytes.extend_from_slice(&self.created_observed.as_nanos().to_be_bytes());
        bytes.extend_from_slice(&self.rotation_max_duration_nanos.to_be_bytes());
        bytes.extend_from_slice(&self.rotation_max_size.to_be_bytes());
        bytes.extend_from_slice(&crc32fast::hash(&bytes).to_be_bytes());
        debug_assert_eq!(bytes.len(), SEGMENT_HEADER_LEN);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> krometrail_core::Result<Self> {
        if bytes.len() != SEGMENT_HEADER_LEN {
            return Err(persistence_error(format!(
                "segment header must be {SEGMENT_HEADER_LEN} bytes, observed {}",
                bytes.len()
            )));
        }
        let (covered, checksum_bytes) = bytes.split_at(SEGMENT_HEADER_LEN - 4);
        let observed_checksum = u32::from_be_bytes(checksum_bytes.try_into().expect("four bytes"));
        let expected_checksum = crc32fast::hash(covered);
        if observed_checksum != expected_checksum {
            return Err(persistence_error("segment header CRC32 mismatch"));
        }

        let mut reader = WireReader::new(covered);
        if reader.read_bytes(4)? != SEGMENT_MAGIC {
            return Err(persistence_error("segment header magic is invalid"));
        }
        let version = reader.read_u16()?;
        if version != FORMAT_VERSION {
            return Err(persistence_error(format!(
                "unsupported segment format version: expected {FORMAT_VERSION}, observed {version}"
            )));
        }
        let header = Self {
            segment_id: SegmentId::from_uuid(reader.read_uuid()?),
            session_id: SessionId::from_uuid(reader.read_uuid()?),
            target_id: TargetId::from_uuid(reader.read_uuid()?),
            start_session_time: SessionTime::from_nanos(reader.read_u64()?),
            created_observed: ObservedTime::from_nanos(reader.read_u64()?),
            rotation_max_duration_nanos: reader.read_u64()?,
            rotation_max_size: reader.read_u64()?,
        };
        debug_assert_eq!(reader.remaining(), 0);
        Ok(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::ErrorCode;
    use uuid::Uuid;

    fn header() -> SegmentHeader {
        SegmentHeader::new(
            SegmentId::from_uuid(Uuid::from_u128(1)),
            SessionId::from_uuid(Uuid::from_u128(2)),
            TargetId::from_uuid(Uuid::from_u128(3)),
            SessionTime::from_nanos(4),
            ObservedTime::from_nanos(5),
            6,
            7,
        )
    }

    #[test]
    fn header_round_trip_is_fixed_width_big_endian() {
        let encoded = header().encode();
        assert_eq!(encoded.len(), SEGMENT_HEADER_LEN);
        assert_eq!(&encoded[..6], &[b'K', b'T', b'S', b'F', 0, 1]);
        assert_eq!(&encoded[62..70], &[0, 0, 0, 0, 0, 0, 0, 5]);
        assert_eq!(SegmentHeader::decode(&encoded).unwrap(), header());
    }

    #[test]
    fn header_rejects_version_and_crc_corruption() {
        for version in [0_u16, 2] {
            let mut wrong_version = header().encode();
            wrong_version[4..6].copy_from_slice(&version.to_be_bytes());
            let crc = crc32fast::hash(&wrong_version[..SEGMENT_HEADER_LEN - 4]);
            wrong_version[SEGMENT_HEADER_LEN - 4..].copy_from_slice(&crc.to_be_bytes());
            let error = SegmentHeader::decode(&wrong_version).unwrap_err();
            assert_eq!(error.code, ErrorCode::PersistenceFailed);
            assert!(
                error
                    .message
                    .as_str()
                    .contains(&format!("expected 1, observed {version}"))
            );
        }

        let mut corrupt = header().encode();
        corrupt[20] ^= 1;
        assert!(
            SegmentHeader::decode(&corrupt)
                .unwrap_err()
                .message
                .as_str()
                .contains("CRC32")
        );
    }
}
