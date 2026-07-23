use std::fs::File;

use krometrail_core::{
    ByteOffset, CaptureOrdinal, CaptureWarning, CapturedFrame, DeviceScaleFactor, EncodedFrame,
    ErrorCode, FrameAddress, FrameAvailability, FrameId, FrameSource, ImageFormat, KrometrailError,
    NonEmptyText, ObservationKind, ObservationPayloadRef, PortFuture, RetrieveSourceFrameRequest,
    SessionId, SessionRange, SourceFrameBatch, SourceFrameList, SourceFrameRead,
    SourceFramesRequest, TargetId, TimelineObservation,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    FrameWriteCommit, persistence_error,
    segments::{read_frame_from, sealed_segment_path},
};

use super::{
    SqliteIndex, codec,
    range::evicted_ranges,
    segments::{SegmentUsageDelta, StoredAddress, register_segment_tx},
    timeline::append_observation_tx,
};

pub(crate) fn index_frame_tx(
    transaction: &Transaction<'_>,
    frame: &EncodedFrame,
    commit: &FrameWriteCommit,
) -> krometrail_core::Result<SegmentUsageDelta> {
    let mut usage_delta = SegmentUsageDelta::default();
    if let Some(sealed) = &commit.sealed_segment {
        usage_delta = usage_delta.combine(register_segment_tx(transaction, sealed)?)?;
    }
    usage_delta = usage_delta.combine(register_segment_tx(transaction, &commit.active_segment)?)?;
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
    append_observation_tx(transaction, &observation, Some(metadata.capture_ordinal()))?;
    Ok(usage_delta)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FrameReadSnapshot {
    pub metadata: CapturedFrame,
    address: StoredAddress,
}

impl SqliteIndex {
    pub(crate) fn frame_read_snapshots_by_id(
        &self,
        frame_ids: &[FrameId],
    ) -> krometrail_core::Result<Vec<FrameReadSnapshot>> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare(&snapshot_select("f.frame_id=?1", ""))
            .map_err(|_| persistence_error("could not prepare coherent frame lookup"))?;
        frame_ids
            .iter()
            .map(|frame_id| {
                let raw = statement
                    .query_row(
                        params![codec::id(frame_id.as_uuid()).to_vec()],
                        raw_snapshot,
                    )
                    .optional()
                    .map_err(|_| persistence_error("could not query coherent frame metadata"))?
                    .ok_or_else(frame_not_found)?;
                decode_snapshot(raw)
            })
            .collect()
    }

    pub(crate) fn frame_read_snapshots_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> krometrail_core::Result<Vec<FrameReadSnapshot>> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare(&snapshot_select(
                "f.session_id=?1 AND f.target_id=?2 AND f.session_time_be>=?3 AND f.session_time_be<=?4",
                "ORDER BY f.session_time_be ASC, f.capture_ordinal_be ASC",
            ))
            .map_err(|_| persistence_error("could not prepare coherent frame range lookup"))?;
        let rows = statement
            .query_map(
                params![
                    codec::id(session_id.as_uuid()).to_vec(),
                    codec::id(target_id.as_uuid()).to_vec(),
                    codec::u64_blob(range.start().as_nanos()).to_vec(),
                    codec::u64_blob(range.end().as_nanos()).to_vec(),
                ],
                raw_snapshot,
            )
            .map_err(|_| persistence_error("could not query coherent frame range"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read coherent frame range"))?
            .into_iter()
            .map(decode_snapshot)
            .collect()
    }

    pub(crate) fn frame_read_snapshots_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> krometrail_core::Result<Vec<FrameReadSnapshot>> {
        if start > end {
            return Err(KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("frame ordinal range start must not exceed its end")
                    .expect("static frame error is non-empty"),
            ));
        }
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare(&snapshot_select(
                "f.session_id=?1 AND f.target_id=?2 AND f.capture_ordinal_be>=?3 AND f.capture_ordinal_be<=?4",
                "ORDER BY f.capture_ordinal_be ASC, f.session_time_be ASC, f.frame_id ASC",
            ))
            .map_err(|_| persistence_error("could not prepare coherent frame ordinal lookup"))?;
        let rows = statement
            .query_map(
                params![
                    codec::id(session_id.as_uuid()).to_vec(),
                    codec::id(target_id.as_uuid()).to_vec(),
                    codec::u64_blob(start.get()).to_vec(),
                    codec::u64_blob(end.get()).to_vec(),
                ],
                raw_snapshot,
            )
            .map_err(|_| persistence_error("could not query coherent frame ordinal range"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read coherent frame ordinal range"))?
            .into_iter()
            .map(decode_snapshot)
            .collect()
    }

    pub(crate) fn read_frame_snapshot(
        &self,
        snapshot: &FrameReadSnapshot,
    ) -> krometrail_core::Result<EncodedFrame> {
        let frame = self.read_address(snapshot.address.clone())?;
        if frame.metadata() != &snapshot.metadata {
            return Err(persistence_error(
                "encoded source frame metadata changed from its indexed snapshot",
            ));
        }
        Ok(frame)
    }
}

fn snapshot_select(predicate: &str, order: &str) -> String {
    format!(
        "SELECT f.frame_id,f.session_id,f.target_id,f.segment_id,f.byte_offset_be,s.state,\
                f.capture_ordinal_be,f.source_time_be,f.observed_time_be,f.session_time_be,f.format,\
                f.image_width,f.image_height,f.viewport_width,f.viewport_height,f.device_scale,\
                f.warnings_json FROM frames f JOIN segments s USING(segment_id) \
         WHERE {predicate} {order}"
    )
}

struct RawSnapshot {
    address: RawAddress,
    metadata: RawMetadata,
}

fn raw_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSnapshot> {
    Ok(RawSnapshot {
        address: RawAddress {
            frame_id: row.get(0)?,
            session_id: row.get(1)?,
            target_id: row.get(2)?,
            segment_id: row.get(3)?,
            byte_offset: row.get(4)?,
            state: row.get(5)?,
        },
        metadata: RawMetadata {
            frame_id: row.get(0)?,
            session_id: row.get(1)?,
            target_id: row.get(2)?,
            capture_ordinal: row.get(6)?,
            source_time: row.get(7)?,
            observed_time: row.get(8)?,
            session_time: row.get(9)?,
            format: row.get(10)?,
            image_width: row.get(11)?,
            image_height: row.get(12)?,
            viewport_width: row.get(13)?,
            viewport_height: row.get(14)?,
            device_scale: row.get(15)?,
            warnings_json: row.get(16)?,
        },
    })
}

fn decode_snapshot(raw: RawSnapshot) -> krometrail_core::Result<FrameReadSnapshot> {
    Ok(FrameReadSnapshot {
        metadata: decode_metadata(raw.metadata)?,
        address: decode_address(raw.address)?,
    })
}

impl FrameSource for SqliteIndex {
    fn list_source_frames(
        &self,
        _request: SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<SourceFrameList>> {
        Box::pin(std::future::ready(Err(progressive_read_unsupported())))
    }

    fn fetch_source_frames(
        &self,
        _request: SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<SourceFrameBatch>> {
        Box::pin(std::future::ready(Err(progressive_read_unsupported())))
    }

    fn read_source_frame(
        &self,
        _request: RetrieveSourceFrameRequest,
    ) -> PortFuture<'_, krometrail_core::Result<SourceFrameRead>> {
        Box::pin(std::future::ready(Err(progressive_read_unsupported())))
    }

    fn frames_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            let addresses = {
                let connection = self.read_connection()?;
                let mut statement = connection
                    .prepare(
                        "SELECT f.frame_id, f.session_id, f.target_id, f.segment_id, \
                                f.byte_offset_be, s.state \
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

    fn frame_metadata_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        Box::pin(async move {
            let connection = self.read_connection()?;
            let mut statement = connection
                .prepare(
                    "SELECT frame_id, session_id, target_id, capture_ordinal_be, source_time_be, \
                            observed_time_be, session_time_be, format, image_width, image_height, \
                            viewport_width, viewport_height, device_scale, warnings_json \
                     FROM frames WHERE frame_id=?1",
                )
                .map_err(|_| persistence_error("could not prepare frame metadata lookup"))?;
            frame_ids
                .into_iter()
                .map(|frame_id| {
                    let raw = statement
                        .query_row(
                            params![codec::id(frame_id.as_uuid()).to_vec()],
                            raw_metadata,
                        )
                        .optional()
                        .map_err(|_| persistence_error("could not query frame metadata"))?
                        .ok_or_else(frame_not_found)?;
                    decode_metadata(raw)
                })
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
                let connection = self.read_connection()?;
                let mut statement = connection
                    .prepare(
                        "SELECT f.frame_id, f.session_id, f.target_id, f.segment_id, \
                                f.byte_offset_be, s.state \
                         FROM frames f JOIN segments s USING(segment_id) \
                         WHERE f.session_id=?1 AND f.target_id=?2 \
                           AND f.session_time_be>=?3 AND f.session_time_be<=?4 \
                         ORDER BY f.session_time_be ASC, f.capture_ordinal_be ASC",
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

    fn frames_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            if start > end {
                return Err(krometrail_core::KrometrailError::new(
                    ErrorCode::InvalidInput,
                    NonEmptyText::new("frame ordinal range start must not exceed its end")
                        .expect("static frame error is non-empty"),
                ));
            }
            let addresses = {
                let connection = self.read_connection()?;
                let mut statement = connection
                    .prepare(
                        "SELECT f.frame_id, f.session_id, f.target_id, f.segment_id, \
                                f.byte_offset_be, s.state \
                         FROM frames f JOIN segments s USING(segment_id) \
                         WHERE f.session_id=?1 AND f.target_id=?2 \
                           AND f.capture_ordinal_be>=?3 AND f.capture_ordinal_be<=?4 \
                         ORDER BY f.capture_ordinal_be ASC, f.session_time_be ASC, f.frame_id ASC",
                    )
                    .map_err(|_| persistence_error("could not prepare frame ordinal lookup"))?;
                let rows = statement
                    .query_map(
                        params![
                            codec::id(session_id.as_uuid()).to_vec(),
                            codec::id(target_id.as_uuid()).to_vec(),
                            codec::u64_blob(start.get()).to_vec(),
                            codec::u64_blob(end.get()).to_vec(),
                        ],
                        raw_address,
                    )
                    .map_err(|_| persistence_error("could not query frame ordinal range"))?;
                let raw: Vec<_> = rows
                    .collect::<Result<_, _>>()
                    .map_err(|_| persistence_error("could not read frame ordinal addresses"))?;
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

    fn frame_metadata_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        Box::pin(async move {
            let connection = self.read_connection()?;
            let mut statement = connection
                .prepare(
                    "SELECT frame_id, session_id, target_id, capture_ordinal_be, source_time_be, \
                            observed_time_be, session_time_be, format, image_width, image_height, \
                            viewport_width, viewport_height, device_scale, warnings_json \
                     FROM frames WHERE session_id=?1 AND target_id=?2 \
                       AND session_time_be>=?3 AND session_time_be<=?4 \
                     ORDER BY session_time_be ASC, capture_ordinal_be ASC",
                )
                .map_err(|_| persistence_error("could not prepare frame metadata range lookup"))?;
            let rows = statement
                .query_map(
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                        codec::u64_blob(range.start().as_nanos()).to_vec(),
                        codec::u64_blob(range.end().as_nanos()).to_vec(),
                    ],
                    raw_metadata,
                )
                .map_err(|_| persistence_error("could not query frame metadata range"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| persistence_error("could not read frame metadata range"))?
                .into_iter()
                .map(decode_metadata)
                .collect()
        })
    }

    fn frame_metadata_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        Box::pin(async move {
            if start > end {
                return Err(KrometrailError::new(
                    ErrorCode::InvalidInput,
                    NonEmptyText::new("frame ordinal range start must not exceed its end")
                        .expect("static frame error is non-empty"),
                ));
            }
            let connection = self.read_connection()?;
            let mut statement = connection
                .prepare(
                    "SELECT frame_id, session_id, target_id, capture_ordinal_be, source_time_be, \
                            observed_time_be, session_time_be, format, image_width, image_height, \
                            viewport_width, viewport_height, device_scale, warnings_json \
                     FROM frames WHERE session_id=?1 AND target_id=?2 \
                       AND capture_ordinal_be>=?3 AND capture_ordinal_be<=?4 \
                     ORDER BY capture_ordinal_be ASC, session_time_be ASC, frame_id ASC",
                )
                .map_err(|_| {
                    persistence_error("could not prepare frame metadata ordinal lookup")
                })?;
            let rows = statement
                .query_map(
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                        codec::u64_blob(start.get()).to_vec(),
                        codec::u64_blob(end.get()).to_vec(),
                    ],
                    raw_metadata,
                )
                .map_err(|_| persistence_error("could not query frame metadata ordinal range"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| persistence_error("could not read frame metadata ordinal range"))?
                .into_iter()
                .map(decode_metadata)
                .collect()
        })
    }

    fn frame_availability(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAvailability>> {
        Box::pin(async move {
            let mut connection = self.read_connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(|_| persistence_error("could not begin frame availability read"))?;
            let values = [
                codec::id(session_id.as_uuid()).to_vec(),
                codec::id(target_id.as_uuid()).to_vec(),
            ];
            let start: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT session_time_be FROM frames \
                     WHERE session_id=?1 AND target_id=?2 \
                     ORDER BY session_time_be ASC LIMIT 1",
                    params![&values[0], &values[1]],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| persistence_error("could not query earliest frame availability"))?;
            let end: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT session_time_be FROM frames \
                     WHERE session_id=?1 AND target_id=?2 \
                     ORDER BY session_time_be DESC LIMIT 1",
                    params![&values[0], &values[1]],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| persistence_error("could not query latest frame availability"))?;
            let retained_bounds = match (start, end) {
                (Some(start), Some(end)) => Some(
                    SessionRange::new(
                        krometrail_core::SessionTime::from_nanos(codec::decode_u64(&start)?),
                        krometrail_core::SessionTime::from_nanos(codec::decode_u64(&end)?),
                    )
                    .map_err(|_| persistence_error("stored frame availability is invalid"))?,
                ),
                (None, None) => None,
                _ => return Err(persistence_error("stored frame availability is malformed")),
            };
            let evicted_ranges = evicted_ranges(&transaction, session_id, target_id)?;
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit frame availability read"))?;
            FrameAvailability::new(retained_bounds, evicted_ranges)
                .map_err(|_| persistence_error("stored frame availability is invalid"))
        })
    }
}

fn progressive_read_unsupported() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Unsupported,
        NonEmptyText::new("the metadata index is not a coherent progressive frame authority")
            .expect("static frame-source error is non-empty"),
    )
}

struct RawMetadata {
    frame_id: Vec<u8>,
    session_id: Vec<u8>,
    target_id: Vec<u8>,
    capture_ordinal: Vec<u8>,
    source_time: Option<Vec<u8>>,
    observed_time: Vec<u8>,
    session_time: Vec<u8>,
    format: String,
    image_width: i64,
    image_height: i64,
    viewport_width: i64,
    viewport_height: i64,
    device_scale: f64,
    warnings_json: String,
}

fn raw_metadata(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawMetadata> {
    Ok(RawMetadata {
        frame_id: row.get(0)?,
        session_id: row.get(1)?,
        target_id: row.get(2)?,
        capture_ordinal: row.get(3)?,
        source_time: row.get(4)?,
        observed_time: row.get(5)?,
        session_time: row.get(6)?,
        format: row.get(7)?,
        image_width: row.get(8)?,
        image_height: row.get(9)?,
        viewport_width: row.get(10)?,
        viewport_height: row.get(11)?,
        device_scale: row.get(12)?,
        warnings_json: row.get(13)?,
    })
}

fn decode_metadata(raw: RawMetadata) -> krometrail_core::Result<CapturedFrame> {
    let format = match raw.format.as_str() {
        "jpeg" => ImageFormat::Jpeg,
        "png" => ImageFormat::Png,
        _ => return Err(persistence_error("stored frame format is unknown")),
    };
    let dimensions = |width: i64, height: i64| {
        let width = u32::try_from(width)
            .map_err(|_| persistence_error("stored frame dimensions are malformed"))?;
        let height = u32::try_from(height)
            .map_err(|_| persistence_error("stored frame dimensions are malformed"))?;
        krometrail_core::PixelDimensions::new(width, height)
            .map_err(|_| persistence_error("stored frame dimensions are invalid"))
    };
    let warnings: Vec<CaptureWarning> = serde_json::from_str(&raw.warnings_json)
        .map_err(|_| persistence_error("stored frame warnings are malformed"))?;
    CapturedFrame::new(
        FrameId::from_uuid(codec::decode_id(&raw.frame_id)?),
        SessionId::from_uuid(codec::decode_id(&raw.session_id)?),
        TargetId::from_uuid(codec::decode_id(&raw.target_id)?),
        CaptureOrdinal::new(codec::decode_u64(&raw.capture_ordinal)?)
            .map_err(|_| persistence_error("stored frame ordinal is invalid"))?,
        raw.source_time
            .as_deref()
            .map(codec::decode_i128)
            .transpose()?
            .map(krometrail_core::SourceTime::from_nanos),
        krometrail_core::ObservedTime::from_nanos(codec::decode_u64(&raw.observed_time)?),
        krometrail_core::SessionTime::from_nanos(codec::decode_u64(&raw.session_time)?),
        format,
        dimensions(raw.image_width, raw.image_height)?,
        dimensions(raw.viewport_width, raw.viewport_height)?,
        DeviceScaleFactor::new(raw.device_scale)
            .map_err(|_| persistence_error("stored frame scale is invalid"))?,
        warnings,
    )
    .map_err(|_| persistence_error("stored frame metadata is invalid"))
}

struct RawAddress {
    frame_id: Vec<u8>,
    session_id: Vec<u8>,
    target_id: Vec<u8>,
    segment_id: Vec<u8>,
    byte_offset: Vec<u8>,
    state: String,
}

fn raw_address(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawAddress> {
    Ok(RawAddress {
        frame_id: row.get(0)?,
        session_id: row.get(1)?,
        target_id: row.get(2)?,
        segment_id: row.get(3)?,
        byte_offset: row.get(4)?,
        state: row.get(5)?,
    })
}

fn decode_address(raw: RawAddress) -> krometrail_core::Result<StoredAddress> {
    let state = match raw.state.as_str() {
        "open" => crate::SegmentState::Open,
        "sealed" => crate::SegmentState::Sealed,
        _ => return Err(persistence_error("stored segment state is malformed")),
    };
    Ok(StoredAddress {
        frame_id: FrameId::from_uuid(codec::decode_id(&raw.frame_id)?),
        session_id: SessionId::from_uuid(codec::decode_id(&raw.session_id)?),
        target_id: TargetId::from_uuid(codec::decode_id(&raw.target_id)?),
        segment_id: krometrail_core::SegmentId::from_uuid(codec::decode_id(&raw.segment_id)?),
        byte_offset: ByteOffset::new(codec::decode_u64(&raw.byte_offset)?),
        state,
    })
}

impl SqliteIndex {
    fn read_address(&self, stored: StoredAddress) -> krometrail_core::Result<EncodedFrame> {
        let path = self
            .segments_directory()
            .join(crate::segments::segment_file_name(
                stored.segment_id,
                stored.state,
            ));
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
