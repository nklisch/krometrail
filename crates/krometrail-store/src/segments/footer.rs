use krometrail_core::{ObservedTime, SegmentId, SessionTime};

use super::wire::{WireReader, put_uuid};
use crate::persistence_error;

pub const SEALED_FOOTER_MAGIC: &[u8; 4] = b"KTSE";
pub const SEALED_FOOTER_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SealedFooter {
    pub segment_id: SegmentId,
    pub record_count: u64,
    pub total_payload: u64,
    pub first_session_time: SessionTime,
    pub last_session_time: SessionTime,
    pub sealed_observed: ObservedTime,
}

impl SealedFooter {
    pub const fn new(
        segment_id: SegmentId,
        record_count: u64,
        total_payload: u64,
        first_session_time: SessionTime,
        last_session_time: SessionTime,
        sealed_observed: ObservedTime,
    ) -> Self {
        Self {
            segment_id,
            record_count,
            total_payload,
            first_session_time,
            last_session_time,
            sealed_observed,
        }
    }

    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SEALED_FOOTER_LEN);
        bytes.extend_from_slice(SEALED_FOOTER_MAGIC);
        put_uuid(&mut bytes, self.segment_id.as_uuid());
        bytes.extend_from_slice(&self.record_count.to_be_bytes());
        bytes.extend_from_slice(&self.total_payload.to_be_bytes());
        bytes.extend_from_slice(&self.first_session_time.as_nanos().to_be_bytes());
        bytes.extend_from_slice(&self.last_session_time.as_nanos().to_be_bytes());
        bytes.extend_from_slice(&self.sealed_observed.as_nanos().to_be_bytes());
        bytes.extend_from_slice(&crc32fast::hash(&bytes).to_be_bytes());
        debug_assert_eq!(bytes.len(), SEALED_FOOTER_LEN);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> krometrail_core::Result<Self> {
        if bytes.len() != SEALED_FOOTER_LEN {
            return Err(persistence_error(format!(
                "sealed footer must be {SEALED_FOOTER_LEN} bytes, observed {}",
                bytes.len()
            )));
        }
        let (covered, checksum_bytes) = bytes.split_at(SEALED_FOOTER_LEN - 4);
        let observed_checksum = u32::from_be_bytes(checksum_bytes.try_into().expect("four bytes"));
        if observed_checksum != crc32fast::hash(covered) {
            return Err(persistence_error("sealed footer CRC32 mismatch"));
        }

        let mut reader = WireReader::new(covered);
        if reader.read_bytes(4)? != SEALED_FOOTER_MAGIC {
            return Err(persistence_error("sealed footer magic is invalid"));
        }
        let footer = Self {
            segment_id: SegmentId::from_uuid(reader.read_uuid()?),
            record_count: reader.read_u64()?,
            total_payload: reader.read_u64()?,
            first_session_time: SessionTime::from_nanos(reader.read_u64()?),
            last_session_time: SessionTime::from_nanos(reader.read_u64()?),
            sealed_observed: ObservedTime::from_nanos(reader.read_u64()?),
        };
        debug_assert_eq!(reader.remaining(), 0);
        if footer.first_session_time > footer.last_session_time {
            return Err(persistence_error(
                "sealed footer first session time exceeds last session time",
            ));
        }
        Ok(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn footer() -> SealedFooter {
        SealedFooter::new(
            SegmentId::from_uuid(Uuid::from_u128(1)),
            2,
            3,
            SessionTime::from_nanos(4),
            SessionTime::from_nanos(5),
            ObservedTime::from_nanos(6),
        )
    }

    #[test]
    fn footer_round_trips_and_detects_corruption() {
        let encoded = footer().encode();
        assert_eq!(encoded.len(), SEALED_FOOTER_LEN);
        assert_eq!(&encoded[..4], b"KTSE");
        assert_eq!(SealedFooter::decode(&encoded).unwrap(), footer());

        let mut corrupt = encoded;
        corrupt[30] ^= 1;
        assert!(SealedFooter::decode(&corrupt).is_err());
    }
}
