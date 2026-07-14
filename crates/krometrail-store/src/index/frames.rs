use std::fs::File;

use krometrail_core::{
    ByteOffset, EncodedFrame, ErrorCode, FrameAddress, FrameId, FrameSource, ImageFormat,
    KrometrailError, NonEmptyText, ObservationKind, ObservationPayloadRef, PortFuture, SessionId,
    SessionRange, TargetId, TimelineObservation,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    FrameWriteCommit, persistence_error,
    segments::{read_frame_from, sealed_segment_path},
};

use super::{
    SqliteIndex, codec,
    segments::{StoredAddress, register_segment_tx},
    timeline::append_observation_tx,
};

pub(crate) fn index_frame_tx(
    transaction: &Transaction<'_>,
    frame: &EncodedFrame,
    commit: &FrameWriteCommit,
) -> krometrail_core::Result<()> {
    if let Some(sealed) = &commit.sealed_segment {
        register_segment_tx(transaction, sealed)?;
    }
    register_segment_tx(transaction, &commit.active_segment)?;
    let metadata = frame.metadata();
    if commit.active_segment.segment_id != commit.address.segment_id
        || commit.active_segment.session_id != metadata.session_id()
        || commit.active_segment.target_id != metadata.target_id()
    {
        return Err(persistence_error(
            "segment append result does not match frame metadata",
        ));
    }
    let warnings = serde_json::to_string(metadata.warnings())
        .map_err(|_| persistence_error("could not encode frame warnings"))?;
    transaction
        .execute(
            "INSERT INTO frames(\
                frame_id, session_id, target_id, segment_id, byte_offset_be, session_time_be,\
                source_time_be, observed_time_be, capture_ordinal_be, format, image_width,\
                image_height, viewport_width, viewport_height, device_scale, warnings_json\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                codec::id(metadata.id().as_uuid()).to_vec(),
                codec::id(metadata.session_id().as_uuid()).to_vec(),
                codec::id(metadata.target_id().as_uuid()).to_vec(),
                codec::id(commit.address.segment_id.as_uuid()).to_vec(),
                codec::u64_blob(commit.address.byte_offset.get()).to_vec(),
                codec::u64_blob(metadata.session_time().as_nanos()).to_vec(),
                metadata
                    .source_time()
                    .map(|value| codec::i128_blob(value.as_nanos()).to_vec()),
                codec::u64_blob(metadata.observed_time().as_nanos()).to_vec(),
                codec::u64_blob(metadata.capture_ordinal().get()).to_vec(),
                match metadata.format() {
                    ImageFormat::Jpeg => "jpeg",
                    ImageFormat::Png => "png",
                },
                i64::from(metadata.image().width()),
                i64::from(metadata.image().height()),
                i64::from(metadata.viewport().width()),
                i64::from(metadata.viewport().height()),
                metadata.device_scale_factor().get(),
                warnings,
            ],
        )
        .map_err(|_| persistence_error("could not index frame metadata"))?;
    let observation = TimelineObservation::new(
        metadata.session_id(),
        metadata.target_id(),
        metadata.session_time(),
        metadata.source_time(),
        metadata.observed_time(),
        ObservationKind::Frame,
        ObservationPayloadRef::Frame(metadata.id()),
    )
    .map_err(|_| persistence_error("frame cannot form a timeline observation"))?;
    append_observation_tx(transaction, &observation, Some(metadata.capture_ordinal()))
}

impl FrameSource for SqliteIndex {
    fn frames_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            let addresses = {
                let connection = self.connection()?;
                let mut statement = connection
                    .prepare(
                        "SELECT f.frame_id, f.session_id, f.target_id, f.segment_id, \
                                f.byte_offset_be, s.relative_path \
                         FROM frames f JOIN segments s USING(segment_id) WHERE f.frame_id=?1",
                    )
                    .map_err(|_| persistence_error("could not prepare frame address lookup"))?;
                let mut addresses = Vec::with_capacity(frame_ids.len());
                for frame_id in frame_ids {
                    let raw = statement
                        .query_row(params![codec::id(frame_id.as_uuid()).to_vec()], raw_address)
                        .optional()
                        .map_err(|_| persistence_error("could not query frame address"))?
                        .ok_or_else(frame_not_found)?;
                    addresses.push(decode_address(raw)?);
                }
                addresses
            };
            addresses
                .into_iter()
                .map(|address| self.read_address(address))
                .collect()
        })
    }

    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            let addresses = {
                let connection = self.connection()?;
                let mut statement = connection
                    .prepare(
                        "SELECT f.frame_id, f.session_id, f.target_id, f.segment_id, \
                                f.byte_offset_be, s.relative_path \
                         FROM frames f JOIN segments s USING(segment_id) \
                         WHERE f.session_id=?1 AND f.target_id=?2 \
                           AND f.session_time_be>=?3 AND f.session_time_be<=?4 \
                         ORDER BY f.capture_ordinal_be ASC, f.session_time_be ASC, f.frame_id ASC",
                    )
                    .map_err(|_| persistence_error("could not prepare frame range lookup"))?;
                let rows = statement
                    .query_map(
                        params![
                            codec::id(session_id.as_uuid()).to_vec(),
                            codec::id(target_id.as_uuid()).to_vec(),
                            codec::u64_blob(range.start().as_nanos()).to_vec(),
                            codec::u64_blob(range.end().as_nanos()).to_vec(),
                        ],
                        raw_address,
                    )
                    .map_err(|_| persistence_error("could not query frame range"))?;
                let raw: Vec<_> = rows
                    .collect::<Result<_, _>>()
                    .map_err(|_| persistence_error("could not read frame addresses"))?;
                raw.into_iter()
                    .map(decode_address)
                    .collect::<krometrail_core::Result<Vec<_>>>()?
            };
            addresses
                .into_iter()
                .map(|address| self.read_address(address))
                .collect()
        })
    }
}

struct RawAddress {
    frame_id: Vec<u8>,
    session_id: Vec<u8>,
    target_id: Vec<u8>,
    segment_id: Vec<u8>,
    byte_offset: Vec<u8>,
    relative_path: String,
}

fn raw_address(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawAddress> {
    Ok(RawAddress {
        frame_id: row.get(0)?,
        session_id: row.get(1)?,
        target_id: row.get(2)?,
        segment_id: row.get(3)?,
        byte_offset: row.get(4)?,
        relative_path: row.get(5)?,
    })
}

fn decode_address(raw: RawAddress) -> krometrail_core::Result<StoredAddress> {
    if raw.relative_path.is_empty() || raw.relative_path.contains(['/', '\\']) {
        return Err(persistence_error("stored segment path is invalid"));
    }
    Ok(StoredAddress {
        frame_id: FrameId::from_uuid(codec::decode_id(&raw.frame_id)?),
        session_id: SessionId::from_uuid(codec::decode_id(&raw.session_id)?),
        target_id: TargetId::from_uuid(codec::decode_id(&raw.target_id)?),
        segment_id: krometrail_core::SegmentId::from_uuid(codec::decode_id(&raw.segment_id)?),
        byte_offset: ByteOffset::new(codec::decode_u64(&raw.byte_offset)?),
        relative_path: raw.relative_path,
    })
}

impl SqliteIndex {
    fn read_address(&self, stored: StoredAddress) -> krometrail_core::Result<EncodedFrame> {
        let path = self.segments_directory().join(&stored.relative_path);
        let mut file = File::open(path)
            .or_else(|_| {
                File::open(sealed_segment_path(
                    self.segments_directory(),
                    stored.segment_id,
                ))
            })
            .map_err(|_| persistence_error("indexed frame segment is unavailable"))?;
        let frame = read_frame_from(
            &mut file,
            FrameAddress::new(stored.segment_id, stored.byte_offset),
        )?;
        if frame.metadata().id() != stored.frame_id
            || frame.metadata().session_id() != stored.session_id
            || frame.metadata().target_id() != stored.target_id
        {
            return Err(persistence_error(
                "indexed frame does not match its stored address context",
            ));
        }
        Ok(frame)
    }
}

fn frame_not_found() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("requested frame is not retained")
            .expect("static frame error is non-empty"),
    )
}
