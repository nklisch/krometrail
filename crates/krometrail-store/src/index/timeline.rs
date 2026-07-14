use krometrail_core::{
    CaptureOrdinal, ErrorCode, KrometrailError, NonEmptyText, ObservationKind,
    ObservationPayloadRef, ObservedTime, PortFuture, SessionId, SessionRange, SessionTime,
    SourceTime, TargetId, TimelineObservation, TimelineStore,
};
use rusqlite::{OptionalExtension, Transaction, params};

use super::{SqliteIndex, codec, ensure_identity};
use crate::persistence_error;

pub(crate) fn append_observation_tx(
    transaction: &Transaction<'_>,
    observation: &TimelineObservation,
    capture_ordinal: Option<CaptureOrdinal>,
) -> krometrail_core::Result<()> {
    ensure_identity(
        transaction,
        observation.session_id(),
        observation.target_id(),
    )?;
    let payload_json = serde_json::to_string(observation.payload())
        .map_err(|_| persistence_error("could not encode timeline payload reference"))?;
    let sort_key = payload_sort_key(observation.payload());
    if matches!(
        observation.kind(),
        ObservationKind::InteractionBoundary
            | ObservationKind::Navigation
            | ObservationKind::Marker
    ) {
        let selection = "SELECT session_id, target_id, session_time_be, source_time_be, \
                                observed_time_be, kind, payload_json \
                         FROM timeline_observations";
        let existing = if observation.kind() == ObservationKind::InteractionBoundary {
            transaction.query_row(
                &format!(
                    "{selection} WHERE kind=?1 AND payload_sort_key=?2 \
                     AND session_time_be=?3 LIMIT 1"
                ),
                params![
                    observation.kind().as_str(),
                    &sort_key,
                    codec::u64_blob(observation.session_time().as_nanos()).to_vec(),
                ],
                raw_observation,
            )
        } else {
            transaction.query_row(
                &format!("{selection} WHERE kind=?1 AND payload_sort_key=?2 LIMIT 1"),
                params![observation.kind().as_str(), &sort_key],
                raw_observation,
            )
        }
        .optional()
        .map_err(|_| persistence_error("could not validate timeline evidence replay"))?;
        if let Some(existing) = existing {
            if decode_observation(existing)? == *observation {
                return Ok(());
            }
            return Err(persistence_error(
                "timeline evidence identity conflicts with its durable value",
            ));
        }
    }
    transaction
        .execute(
            "INSERT INTO timeline_observations(\
                session_id, target_id, session_time_be, source_time_be, observed_time_be,\
                capture_ordinal_be, kind, payload_json, payload_sort_key\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                codec::id(observation.session_id().as_uuid()).to_vec(),
                codec::id(observation.target_id().as_uuid()).to_vec(),
                codec::u64_blob(observation.session_time().as_nanos()).to_vec(),
                observation
                    .source_time()
                    .map(|value| codec::i128_blob(value.as_nanos()).to_vec()),
                codec::u64_blob(observation.observed_time().as_nanos()).to_vec(),
                capture_ordinal.map(|value| codec::u64_blob(value.get()).to_vec()),
                observation.kind().as_str(),
                payload_json,
                sort_key,
            ],
        )
        .map_err(|_| persistence_error("could not persist timeline metadata"))?;
    Ok(())
}

impl TimelineStore for SqliteIndex {
    fn append(
        &self,
        observation: TimelineObservation,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            if matches!(
                observation.kind(),
                ObservationKind::Frame | ObservationKind::CaptureGap
            ) {
                return Err(authoritative_path_required());
            }
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin timeline persistence"))?;
            append_observation_tx(&transaction, &observation, None)?;
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit timeline metadata"))
        })
    }

    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<TimelineObservation>>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let mut statement = connection
                .prepare(
                    "SELECT session_id, target_id, session_time_be, source_time_be, \
                            observed_time_be, kind, payload_json \
                     FROM timeline_observations \
                     WHERE session_id=?1 AND target_id=?2 \
                       AND session_time_be>=?3 AND session_time_be<=?4 \
                     ORDER BY session_time_be ASC, \
                       CASE WHEN capture_ordinal_be IS NULL THEN 1 ELSE 0 END ASC, \
                       capture_ordinal_be ASC, observed_time_be ASC, kind ASC, \
                       payload_sort_key ASC, observation_id ASC",
                )
                .map_err(|_| persistence_error("could not prepare the timeline range query"))?;
            let rows = statement
                .query_map(
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                        codec::u64_blob(range.start().as_nanos()).to_vec(),
                        codec::u64_blob(range.end().as_nanos()).to_vec(),
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
                .map_err(|_| persistence_error("could not query timeline metadata"))?;
            let raw: Vec<_> = rows
                .collect::<Result<_, _>>()
                .map_err(|_| persistence_error("could not read timeline metadata"))?;
            raw.into_iter().map(decode_observation).collect()
        })
    }
}

fn raw_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawObservation> {
    Ok(RawObservation {
        session_id: row.get(0)?,
        target_id: row.get(1)?,
        session_time: row.get(2)?,
        source_time: row.get(3)?,
        observed_time: row.get(4)?,
        kind: row.get(5)?,
        payload_json: row.get(6)?,
    })
}

pub(crate) struct RawObservation {
    pub(crate) session_id: Vec<u8>,
    pub(crate) target_id: Vec<u8>,
    pub(crate) session_time: Vec<u8>,
    pub(crate) source_time: Option<Vec<u8>>,
    pub(crate) observed_time: Vec<u8>,
    pub(crate) kind: String,
    pub(crate) payload_json: String,
}

pub(crate) fn decode_observation(
    raw: RawObservation,
) -> krometrail_core::Result<TimelineObservation> {
    let kind = ObservationKind::from_stable_name(&raw.kind)
        .ok_or_else(|| persistence_error("stored observation kind is unknown"))?;
    let payload: ObservationPayloadRef = serde_json::from_str(&raw.payload_json)
        .map_err(|_| persistence_error("stored timeline payload reference is malformed"))?;
    TimelineObservation::new(
        SessionId::from_uuid(codec::decode_id(&raw.session_id)?),
        TargetId::from_uuid(codec::decode_id(&raw.target_id)?),
        SessionTime::from_nanos(codec::decode_u64(&raw.session_time)?),
        raw.source_time
            .as_deref()
            .map(codec::decode_i128)
            .transpose()?
            .map(SourceTime::from_nanos),
        ObservedTime::from_nanos(codec::decode_u64(&raw.observed_time)?),
        kind,
        payload,
    )
    .map_err(|_| persistence_error("stored timeline observation is invalid"))
}

fn payload_sort_key(payload: &ObservationPayloadRef) -> Vec<u8> {
    match payload {
        ObservationPayloadRef::Frame(value) => codec::id(value.as_uuid()).to_vec(),
        ObservationPayloadRef::Interaction(value) => codec::id(value.as_uuid()).to_vec(),
        ObservationPayloadRef::Navigation(value) => codec::id(value.as_uuid()).to_vec(),
        ObservationPayloadRef::Gap(value) => codec::id(value.as_uuid()).to_vec(),
        ObservationPayloadRef::Marker(value) => codec::id(value.as_uuid()).to_vec(),
        ObservationPayloadRef::External(value) => value.as_bytes().to_vec(),
    }
}

fn authoritative_path_required() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(
            "frame and capture-gap observations require their authoritative persistence path",
        )
        .expect("static timeline error is non-empty"),
    )
}
