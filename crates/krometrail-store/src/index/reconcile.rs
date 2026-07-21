use krometrail_core::{
    ByteOffset, EncodedFrame, FrameId, SegmentId, SessionId, SessionTime, TargetId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{FrameWriteCommit, SegmentRegistration, SegmentState, persistence_error};

use super::{codec, frames::index_frame_tx};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredSegment {
    pub registration: SegmentRegistration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedFrame {
    pub frame_id: FrameId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub byte_offset: ByteOffset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SegmentUsage {
    pub object_key: Box<[u8]>,
    pub session_id: Option<SessionId>,
    pub byte_len: u64,
}

pub(crate) fn list_segments(
    connection: &Connection,
) -> krometrail_core::Result<Vec<StoredSegment>> {
    let mut statement = connection
        .prepare(
            "SELECT segment_id, session_id, target_id, state, start_time_be, \
                    end_time_be, file_bytes_be, payload_bytes_be, record_count_be \
             FROM segments ORDER BY segment_id",
        )
        .map_err(|_| persistence_error("could not prepare recovery segment lookup"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
                row.get::<_, Vec<u8>>(6)?,
                row.get::<_, Vec<u8>>(7)?,
                row.get::<_, Vec<u8>>(8)?,
            ))
        })
        .map_err(|_| persistence_error("could not query recovery segments"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read recovery segments"))?
        .into_iter()
        .map(
            |(
                segment_id,
                session_id,
                target_id,
                state,
                start_time,
                end_time,
                file_bytes,
                payload_bytes,
                record_count,
            )| {
                let state = match state.as_str() {
                    "open" => SegmentState::Open,
                    "sealed" => SegmentState::Sealed,
                    _ => return Err(persistence_error("stored segment state is malformed")),
                };
                Ok(StoredSegment {
                    registration: SegmentRegistration {
                        segment_id: SegmentId::from_uuid(codec::decode_id(&segment_id)?),
                        session_id: SessionId::from_uuid(codec::decode_id(&session_id)?),
                        target_id: TargetId::from_uuid(codec::decode_id(&target_id)?),
                        state,
                        start_time: SessionTime::from_nanos(codec::decode_u64(&start_time)?),
                        end_time: end_time
                            .as_deref()
                            .map(codec::decode_u64)
                            .transpose()?
                            .map(SessionTime::from_nanos),
                        file_bytes: codec::decode_u64(&file_bytes)?,
                        payload_bytes: codec::decode_u64(&payload_bytes)?,
                        record_count: codec::decode_u64(&record_count)?,
                    },
                })
            },
        )
        .collect()
}

pub(crate) fn indexed_frames(
    connection: &Connection,
    segment_id: SegmentId,
) -> krometrail_core::Result<Vec<IndexedFrame>> {
    let mut statement = connection
        .prepare(
            "SELECT frame_id, session_id, target_id, byte_offset_be FROM frames \
             WHERE segment_id=?1 ORDER BY byte_offset_be, frame_id",
        )
        .map_err(|_| persistence_error("could not prepare recovery frame lookup"))?;
    let rows = statement
        .query_map(params![codec::id(segment_id.as_uuid()).to_vec()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|_| persistence_error("could not query recovery frames"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read recovery frames"))?
        .into_iter()
        .map(|(frame_id, session_id, target_id, byte_offset)| {
            Ok(IndexedFrame {
                frame_id: FrameId::from_uuid(codec::decode_id(&frame_id)?),
                session_id: SessionId::from_uuid(codec::decode_id(&session_id)?),
                target_id: TargetId::from_uuid(codec::decode_id(&target_id)?),
                byte_offset: ByteOffset::new(codec::decode_u64(&byte_offset)?),
            })
        })
        .collect()
}

pub(crate) fn frame_exists(
    connection: &Connection,
    frame_id: FrameId,
) -> krometrail_core::Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM frames WHERE frame_id=?1",
            params![codec::id(frame_id.as_uuid()).to_vec()],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| persistence_error("could not check recovered frame metadata"))
}

pub(crate) fn upsert_recovered_frame_tx(
    transaction: &Transaction<'_>,
    frame: &EncodedFrame,
    commit: &FrameWriteCommit,
) -> krometrail_core::Result<bool> {
    if frame_exists(transaction, frame.metadata().id())? {
        return Ok(false);
    }
    index_frame_tx(transaction, frame, commit)?;
    Ok(true)
}

pub(crate) fn register_segment_tx(
    transaction: &Transaction<'_>,
    registration: &SegmentRegistration,
) -> krometrail_core::Result<()> {
    super::segments::register_segment_tx(transaction, registration)
}

pub(crate) fn list_segment_usage(
    connection: &Connection,
) -> krometrail_core::Result<Vec<SegmentUsage>> {
    let mut statement = connection
        .prepare(
            "SELECT object_key, session_id, byte_len_be FROM usage \
             WHERE class='segment' ORDER BY object_key",
        )
        .map_err(|_| persistence_error("could not prepare recovery usage lookup"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|_| persistence_error("could not query recovery usage"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read recovery usage"))?
        .into_iter()
        .map(|(object_key, session_id, byte_len)| {
            Ok(SegmentUsage {
                object_key: object_key.into_boxed_slice(),
                session_id: session_id
                    .as_deref()
                    .map(codec::decode_id)
                    .transpose()?
                    .map(SessionId::from_uuid),
                byte_len: codec::decode_u64(&byte_len)?,
            })
        })
        .collect()
}

pub(crate) fn segment_usage_key(segment_id: SegmentId) -> Box<[u8]> {
    codec::id(segment_id.as_uuid()).to_vec().into_boxed_slice()
}
