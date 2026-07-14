use krometrail_core::{SegmentId, SessionId, TargetId};
use rusqlite::{Transaction, params};

use crate::{SegmentRegistration, persistence_error};

use super::{codec, ensure_identity};

pub(crate) fn register_segment_tx(
    transaction: &Transaction<'_>,
    registration: &SegmentRegistration,
) -> krometrail_core::Result<()> {
    ensure_identity(transaction, registration.session_id, registration.target_id)?;
    let relative_path = registration
        .relative_path
        .to_str()
        .filter(|value| !value.is_empty() && !value.contains(['/', '\\']))
        .ok_or_else(|| persistence_error("segment registration path is not a file name"))?;
    transaction
        .execute(
            "INSERT INTO segments(\
                segment_id, session_id, target_id, state, relative_path, start_time_be,\
                end_time_be, file_bytes_be, payload_bytes_be, record_count_be\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(segment_id) DO UPDATE SET \
                state=excluded.state, relative_path=excluded.relative_path, \
                end_time_be=excluded.end_time_be, file_bytes_be=excluded.file_bytes_be, \
                payload_bytes_be=excluded.payload_bytes_be, record_count_be=excluded.record_count_be",
            params![
                codec::id(registration.segment_id.as_uuid()).to_vec(),
                codec::id(registration.session_id.as_uuid()).to_vec(),
                codec::id(registration.target_id.as_uuid()).to_vec(),
                registration.state.as_str(),
                relative_path,
                codec::u64_blob(registration.start_time.as_nanos()).to_vec(),
                registration
                    .end_time
                    .map(|value| codec::u64_blob(value.as_nanos()).to_vec()),
                codec::u64_blob(registration.file_bytes).to_vec(),
                codec::u64_blob(registration.payload_bytes).to_vec(),
                codec::u64_blob(registration.record_count).to_vec(),
            ],
        )
        .map_err(|_| persistence_error("could not register segment metadata"))?;
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct StoredAddress {
    pub frame_id: krometrail_core::FrameId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub segment_id: SegmentId,
    pub byte_offset: krometrail_core::ByteOffset,
    pub relative_path: String,
}
