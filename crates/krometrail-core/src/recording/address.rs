use serde::{Deserialize, Serialize};

use crate::ids::SegmentId;

/// Byte offset of a frame record's start within a segment file.
///
/// The offset points at the record-kind byte so a reader can parse the complete
/// self-describing record after one seek.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteOffset(u64);

impl ByteOffset {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable location of a durably written frame record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FrameAddress {
    pub segment_id: SegmentId,
    pub byte_offset: ByteOffset,
}

impl FrameAddress {
    pub const fn new(segment_id: SegmentId, byte_offset: ByteOffset) -> Self {
        Self {
            segment_id,
            byte_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_is_copy_and_round_trips_through_serde() {
        let address = FrameAddress::new(
            SegmentId::from_uuid(uuid::Uuid::from_u128(42)),
            ByteOffset::new(0),
        );
        let copy = address;
        assert_eq!(copy.byte_offset.get(), 0);

        let encoded = serde_json::to_string(&address).unwrap();
        assert_eq!(
            serde_json::from_str::<FrameAddress>(&encoded).unwrap(),
            address
        );
    }
}
