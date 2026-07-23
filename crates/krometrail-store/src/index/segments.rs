use krometrail_core::{SegmentId, SessionId, TargetId};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::{SegmentRegistration, persistence_error};

use super::{codec, ensure_identity};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SegmentUsageDelta {
    pub removed: u64,
    pub added: u64,
}

impl SegmentUsageDelta {
    pub(crate) fn from_transition(before: Option<u64>, after: u64) -> Self {
        match before {
            Some(before) if before >= after => Self {
                removed: before - after,
                added: 0,
            },
            Some(before) => Self {
                removed: 0,
                added: after - before,
            },
            None => Self {
                removed: 0,
                added: after,
            },
        }
    }

    pub(crate) fn combine(self, other: Self) -> krometrail_core::Result<Self> {
        Ok(Self {
            removed: self
                .removed
                .checked_add(other.removed)
                .ok_or_else(|| crate::persistence_error("segment usage delta overflow"))?,
            added: self
                .added
                .checked_add(other.added)
                .ok_or_else(|| crate::persistence_error("segment usage delta overflow"))?,
        })
    }
}

pub(crate) fn register_segment_tx(
    transaction: &Transaction<'_>,
    registration: &SegmentRegistration,
) -> krometrail_core::Result<SegmentUsageDelta> {
    ensure_identity(transaction, registration.session_id, registration.target_id)?;
    let segment_key = codec::id(registration.segment_id.as_uuid()).to_vec();
    let previous_bytes: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT byte_len_be FROM usage WHERE class='segment' AND object_key=?1",
            params![&segment_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| persistence_error("could not query segment usage"))?;
    let existing_sequence: Option<i64> = transaction
        .query_row(
            "SELECT retention_sequence FROM segments WHERE segment_id=?1",
            params![&segment_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| persistence_error("could not query segment retention sequence"))?;
    let retention_sequence = if let Some(sequence) = existing_sequence {
        sequence
    } else {
        let sequence: i64 = transaction
            .query_row(
                "SELECT next_value FROM retention_sequence WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| persistence_error("could not allocate segment retention sequence"))?;
        transaction
            .execute(
                "UPDATE retention_sequence SET next_value=next_value+1 WHERE singleton=1",
                [],
            )
            .map_err(|_| persistence_error("could not advance segment retention sequence"))?;
        sequence
    };
    transaction
        .execute(
            "INSERT INTO segments(\
                segment_id, session_id, target_id, state, start_time_be,\
                end_time_be, file_bytes_be, payload_bytes_be, record_count_be, retention_sequence\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(segment_id) DO UPDATE SET \
                state=excluded.state, \
                end_time_be=excluded.end_time_be, file_bytes_be=excluded.file_bytes_be, \
                payload_bytes_be=excluded.payload_bytes_be, record_count_be=excluded.record_count_be",
            params![
                &segment_key,
                codec::id(registration.session_id.as_uuid()).to_vec(),
                codec::id(registration.target_id.as_uuid()).to_vec(),
                registration.state.as_str(),
                codec::u64_blob(registration.start_time.as_nanos()).to_vec(),
                registration
                    .end_time
                    .map(|value| codec::u64_blob(value.as_nanos()).to_vec()),
                codec::u64_blob(registration.file_bytes).to_vec(),
                codec::u64_blob(registration.payload_bytes).to_vec(),
                codec::u64_blob(registration.record_count).to_vec(),
                retention_sequence,
            ],
        )
        .map_err(|_| persistence_error("could not register segment metadata"))?;
    transaction
        .execute(
            "INSERT INTO usage(class, object_key, session_id, byte_len_be) \
             VALUES ('segment', ?1, ?2, ?3) \
             ON CONFLICT(class, object_key) DO UPDATE SET \
                session_id=excluded.session_id, byte_len_be=excluded.byte_len_be",
            params![
                segment_key,
                codec::id(registration.session_id.as_uuid()).to_vec(),
                codec::u64_blob(registration.file_bytes).to_vec(),
            ],
        )
        .map_err(|_| persistence_error("could not register segment usage"))?;
    let previous_bytes = previous_bytes
        .as_deref()
        .map(codec::decode_u64)
        .transpose()?;
    Ok(SegmentUsageDelta::from_transition(
        previous_bytes,
        registration.file_bytes,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredAddress {
    pub frame_id: krometrail_core::FrameId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub segment_id: SegmentId,
    pub byte_offset: krometrail_core::ByteOffset,
    pub state: crate::SegmentState,
}
