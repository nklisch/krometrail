use krometrail_core::{
    AnchorScope, ObservationKind, ObservationPayloadRef, PortFuture, SessionId, SessionRange,
    SessionTime, TargetId, TimelineAnchorSource, TimelineObservation,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::{
    SqliteIndex, codec,
    timeline::{RawObservation, decode_observation},
};
use crate::persistence_error;

pub(crate) fn record_evicted_frame_range_tx(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
) -> krometrail_core::Result<()> {
    let session_key = codec::id(session_id.as_uuid()).to_vec();
    let target_key = codec::id(target_id.as_uuid()).to_vec();
    let mut statement = transaction
        .prepare(
            "SELECT eviction_id, start_time_be, end_time_be FROM evicted_frame_ranges \
             WHERE session_id=?1 AND target_id=?2 ORDER BY start_time_be, end_time_be, eviction_id",
        )
        .map_err(|_| persistence_error("could not prepare eviction-range coalescing"))?;
    let rows = statement
        .query_map(params![&session_key, &target_key], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|_| persistence_error("could not query eviction ranges"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read eviction ranges"))?;
    drop(statement);

    let mut start = range.start().as_nanos();
    let mut end = range.end().as_nanos();
    let mut merged_ids = Vec::new();
    for (id, raw_start, raw_end) in rows {
        let existing_start = codec::decode_u64(&raw_start)?;
        let existing_end = codec::decode_u64(&raw_end)?;
        if existing_start <= end.saturating_add(1) && start <= existing_end.saturating_add(1) {
            start = start.min(existing_start);
            end = end.max(existing_end);
            merged_ids.push(id);
        }
    }
    for id in merged_ids {
        transaction
            .execute(
                "DELETE FROM evicted_frame_ranges WHERE eviction_id=?1",
                params![id],
            )
            .map_err(|_| persistence_error("could not compact eviction ranges"))?;
    }
    transaction
        .execute(
            "INSERT OR IGNORE INTO evicted_frame_ranges(\
                session_id, target_id, start_time_be, end_time_be\
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                session_key,
                target_key,
                codec::u64_blob(start).to_vec(),
                codec::u64_blob(end).to_vec(),
            ],
        )
        .map_err(|_| persistence_error("could not persist eviction range"))?;
    Ok(())
}

pub(crate) fn evicted_ranges(
    connection: &Connection,
    session_id: SessionId,
    target_id: TargetId,
) -> krometrail_core::Result<Vec<SessionRange>> {
    let mut statement = connection
        .prepare(
            "SELECT start_time_be, end_time_be FROM evicted_frame_ranges \
             WHERE session_id=?1 AND target_id=?2 \
             ORDER BY start_time_be, end_time_be, eviction_id",
        )
        .map_err(|_| persistence_error("could not prepare eviction-range lookup"))?;
    let rows = statement
        .query_map(
            params![
                codec::id(session_id.as_uuid()).to_vec(),
                codec::id(target_id.as_uuid()).to_vec(),
            ],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .map_err(|_| persistence_error("could not query eviction ranges"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read eviction ranges"))?;
    rows.into_iter()
        .map(|(start, end)| {
            SessionRange::new(
                SessionTime::from_nanos(codec::decode_u64(&start)?),
                SessionTime::from_nanos(codec::decode_u64(&end)?),
            )
            .map_err(|_| persistence_error("stored eviction range is invalid"))
        })
        .collect()
}

impl TimelineAnchorSource for SqliteIndex {
    fn observation_for_payload(
        &self,
        _scope: AnchorScope,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> PortFuture<'_, krometrail_core::Result<Option<TimelineObservation>>> {
        Box::pin(async move {
            if !matches!(kind, ObservationKind::Marker | ObservationKind::Navigation) {
                return Err(krometrail_core::KrometrailError::new(
                    krometrail_core::ErrorCode::InvalidInput,
                    krometrail_core::NonEmptyText::new(
                        "range anchor lookup supports only marker and navigation observations",
                    )
                    .expect("static range error is non-empty"),
                ));
            }
            let compatible = matches!(
                (&kind, &payload),
                (ObservationKind::Marker, ObservationPayloadRef::Marker(_))
                    | (
                        ObservationKind::Navigation,
                        ObservationPayloadRef::Navigation(_)
                    )
            );
            if !compatible {
                return Err(krometrail_core::KrometrailError::new(
                    krometrail_core::ErrorCode::InvalidInput,
                    krometrail_core::NonEmptyText::new(
                        "range anchor payload does not match its observation kind",
                    )
                    .expect("static range error is non-empty"),
                ));
            }
            let payload_json = serde_json::to_string(&payload)
                .map_err(|_| persistence_error("could not encode range anchor payload"))?;
            let connection = self.connection()?;
            let raw = connection
                .query_row(
                    "SELECT session_id, target_id, session_time_be, source_time_be, \
                            observed_time_be, kind, payload_json \
                     FROM timeline_observations \
                     WHERE kind=?1 AND payload_json=?2 \
                     ORDER BY session_time_be ASC, observed_time_be ASC, observation_id ASC \
                     LIMIT 1",
                    params![kind.as_str(), payload_json,],
                    |row| {
                        Ok(RawObservation {
                            session_id: row.get(0)?,
                            target_id: row.get(1)?,
                            session_time: row.get(2)?,
                            source_time: row.get(3)?,
                            observed_time: row.get(4)?,
                            kind: row.get(5)?,
                            payload_json: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(|_| persistence_error("could not query range anchor"))?;
            raw.map(decode_observation).transpose()
        })
    }

    fn latest_observation(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        kind: ObservationKind,
    ) -> PortFuture<'_, krometrail_core::Result<Option<TimelineObservation>>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let raw = connection
                .query_row(
                    "SELECT session_id, target_id, session_time_be, source_time_be, \
                            observed_time_be, kind, payload_json \
                     FROM timeline_observations \
                     WHERE session_id=?1 AND target_id=?2 AND kind=?3 \
                     ORDER BY session_time_be DESC, observed_time_be DESC, observation_id DESC \
                     LIMIT 1",
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                        kind.as_str(),
                    ],
                    |row| {
                        Ok(RawObservation {
                            session_id: row.get(0)?,
                            target_id: row.get(1)?,
                            session_time: row.get(2)?,
                            source_time: row.get(3)?,
                            observed_time: row.get(4)?,
                            kind: row.get(5)?,
                            payload_json: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(|_| persistence_error("could not query latest range anchor"))?;
            raw.map(decode_observation).transpose()
        })
    }
}
