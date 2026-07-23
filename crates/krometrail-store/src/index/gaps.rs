use std::num::NonZeroU64;

use krometrail_core::{
    CaptureGap, CaptureGapReason, CaptureGapStore, GapId, ObservationKind, ObservationPayloadRef,
    ObservedTime, PortFuture, SessionId, SessionRange, SessionTime, TargetId, TimelineObservation,
};
use rusqlite::params;

use super::{SqliteIndex, codec, ensure_identity, timeline::append_observation_tx};
use crate::persistence_error;

impl CaptureGapStore for SqliteIndex {
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin capture-gap persistence"))?;
            ensure_identity(&transaction, gap.session_id(), gap.target_id())?;
            transaction
                .execute(
                    "INSERT INTO capture_gaps(\
                        gap_id, session_id, target_id, start_time_be, end_time_be,\
                        observed_time_be, reason, estimated_missing_be, detail\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        codec::id(gap.id().as_uuid()).to_vec(),
                        codec::id(gap.session_id().as_uuid()).to_vec(),
                        codec::id(gap.target_id().as_uuid()).to_vec(),
                        codec::u64_blob(gap.range().start().as_nanos()).to_vec(),
                        codec::u64_blob(gap.range().end().as_nanos()).to_vec(),
                        codec::u64_blob(gap.observed_time().as_nanos()).to_vec(),
                        gap.reason().as_str(),
                        gap.estimated_missing_frames()
                            .map(|value| codec::u64_blob(value.get()).to_vec()),
                        gap.detail(),
                    ],
                )
                .map_err(|_| persistence_error("could not persist capture-gap metadata"))?;
            let observation = TimelineObservation::new(
                gap.session_id(),
                gap.target_id(),
                gap.range().start(),
                None,
                gap.observed_time(),
                ObservationKind::CaptureGap,
                ObservationPayloadRef::Gap(gap.id()),
            )
            .map_err(|_| persistence_error("capture gap cannot form a timeline observation"))?;
            append_observation_tx(&transaction, &observation, None)?;
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit capture-gap metadata"))
        })
    }

    fn gaps(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CaptureGap>>> {
        Box::pin(async move {
            let connection = self.read_connection()?;
            let mut statement = connection
                .prepare(
                    "SELECT gap_id, session_id, target_id, start_time_be, end_time_be, \
                            observed_time_be, reason, estimated_missing_be, detail \
                     FROM capture_gaps \
                     WHERE session_id=?1 AND target_id=?2 \
                       AND start_time_be<=?3 AND end_time_be>=?4 \
                     ORDER BY start_time_be ASC, end_time_be ASC, gap_id ASC",
                )
                .map_err(|_| persistence_error("could not prepare the capture-gap range query"))?;
            let rows = statement
                .query_map(
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                        codec::u64_blob(range.end().as_nanos()).to_vec(),
                        codec::u64_blob(range.start().as_nanos()).to_vec(),
                    ],
                    |row| {
                        Ok(RawGap {
                            id: row.get(0)?,
                            session_id: row.get(1)?,
                            target_id: row.get(2)?,
                            start: row.get(3)?,
                            end: row.get(4)?,
                            observed: row.get(5)?,
                            reason: row.get(6)?,
                            estimated: row.get(7)?,
                            detail: row.get(8)?,
                        })
                    },
                )
                .map_err(|_| persistence_error("could not query capture-gap metadata"))?;
            let raw: Vec<_> = rows
                .collect::<Result<_, _>>()
                .map_err(|_| persistence_error("could not read capture-gap metadata"))?;
            raw.into_iter().map(decode_gap).collect()
        })
    }
}

struct RawGap {
    id: Vec<u8>,
    session_id: Vec<u8>,
    target_id: Vec<u8>,
    start: Vec<u8>,
    end: Vec<u8>,
    observed: Vec<u8>,
    reason: String,
    estimated: Option<Vec<u8>>,
    detail: Option<String>,
}

fn decode_gap(raw: RawGap) -> krometrail_core::Result<CaptureGap> {
    let reason = CaptureGapReason::ALL
        .iter()
        .copied()
        .find(|candidate| candidate.as_str() == raw.reason)
        .ok_or_else(|| persistence_error("stored capture-gap reason is unknown"))?;
    let estimated = raw
        .estimated
        .as_deref()
        .map(codec::decode_u64)
        .transpose()?
        .and_then(NonZeroU64::new);
    CaptureGap::new(
        GapId::from_uuid(codec::decode_id(&raw.id)?),
        SessionId::from_uuid(codec::decode_id(&raw.session_id)?),
        TargetId::from_uuid(codec::decode_id(&raw.target_id)?),
        SessionRange::new(
            SessionTime::from_nanos(codec::decode_u64(&raw.start)?),
            SessionTime::from_nanos(codec::decode_u64(&raw.end)?),
        )
        .map_err(|_| persistence_error("stored capture-gap range is invalid"))?,
        ObservedTime::from_nanos(codec::decode_u64(&raw.observed)?),
        reason,
        estimated,
        raw.detail,
    )
    .map_err(|_| persistence_error("stored capture gap is invalid"))
}
