use krometrail_core::{
    CaptureOrdinal, ErrorCode, KrometrailError, NonEmptyText, ObservationKind,
    ObservationPayloadRef, ObservedTime, PortFuture, SessionId, SessionRange, SessionTime,
    SourceTime, TargetId, TimelineObservation, TimelineRangeQuery, TimelineRangeSlice,
    TimelineStore,
};
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};

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
                ObservationKind::Frame
                    | ObservationKind::CaptureGap
                    | ObservationKind::BrowserEvent
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
                    raw_observation,
                )
                .map_err(|_| persistence_error("could not query timeline metadata"))?;
            let raw: Vec<_> = rows
                .collect::<Result<_, _>>()
                .map_err(|_| persistence_error("could not read timeline metadata"))?;
            raw.into_iter().map(decode_observation).collect()
        })
    }

    fn selected_range(
        &self,
        query: TimelineRangeQuery,
    ) -> PortFuture<'_, krometrail_core::Result<TimelineRangeSlice>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let (where_sql, kind_names) = selected_range_sql(&query);
            // Heterogeneous parameter list: scope/range are blobs, kinds are text.
            let scope_params: &[&dyn rusqlite::ToSql] = &[
                &codec::id(query.session_id.as_uuid()).to_vec(),
                &codec::id(query.target_id.as_uuid()).to_vec(),
                &codec::u64_blob(query.range.start().as_nanos()).to_vec(),
                &codec::u64_blob(query.range.end().as_nanos()).to_vec(),
            ];
            let kind_params: Vec<String> =
                kind_names.iter().map(|name| (*name).to_string()).collect();
            let count_params: Vec<&dyn rusqlite::ToSql> = scope_params
                .iter()
                .copied()
                .chain(kind_params.iter().map(|s| s as &dyn rusqlite::ToSql))
                .collect();
            // Count exact matches first, independent of the row limit.
            let matched_count: u64 = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM timeline_observations WHERE {where_sql}"),
                    params_from_iter(count_params.iter()),
                    |row| row.get::<_, i64>(0),
                )
                .map(|value| value.max(0) as u64)
                .map_err(|_| persistence_error("could not count timeline evidence"))?;
            let limit = i64::try_from(usize::from(query.limit.get()))
                .map_err(|_| persistence_error("timeline range limit overflows i64"))?;
            let mut statement = connection
                .prepare(&format!(
                    "SELECT session_id, target_id, session_time_be, source_time_be, \
                            observed_time_be, kind, payload_json \
                     FROM timeline_observations \
                     WHERE {where_sql} \
                     ORDER BY session_time_be ASC, \
                       CASE WHEN capture_ordinal_be IS NULL THEN 1 ELSE 0 END ASC, \
                       capture_ordinal_be ASC, observed_time_be ASC, kind ASC, \
                       payload_sort_key ASC, observation_id ASC \
                     LIMIT ?"
                ))
                .map_err(|_| persistence_error("could not prepare the bounded timeline query"))?;
            let row_params: Vec<&dyn rusqlite::ToSql> = scope_params
                .iter()
                .copied()
                .chain(kind_params.iter().map(|s| s as &dyn rusqlite::ToSql))
                .chain(std::iter::once(&limit as &dyn rusqlite::ToSql))
                .collect();
            let rows = statement
                .query_map(params_from_iter(row_params.iter()), raw_observation)
                .map_err(|_| persistence_error("could not query bounded timeline metadata"))?;
            let raw: Vec<_> = rows
                .collect::<Result<_, _>>()
                .map_err(|_| persistence_error("could not read bounded timeline metadata"))?;
            let observations: Vec<TimelineObservation> = raw
                .into_iter()
                .map(decode_observation)
                .collect::<Result<_, _>>()?;
            let returned_count = observations.len() as u64;
            Ok(TimelineRangeSlice {
                matched_count,
                observations,
                truncated: matched_count > returned_count,
            })
        })
    }
}

/// Builds the WHERE clause text and the ordered kind stable-name list shared by
/// the count and row queries.
///
/// The kind filter is applied at SQL selection time so high-volume kinds
/// (such as browser events) never enter the result when not requested.
fn selected_range_sql(query: &TimelineRangeQuery) -> (String, Vec<&'static str>) {
    let placeholders = query
        .kinds
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "session_id=? AND target_id=? AND session_time_be>=? AND session_time_be<=? \
         AND kind IN ({placeholders})"
    );
    let kind_names = query.kind_names();
    (sql, kind_names)
}

pub(crate) fn raw_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawObservation> {
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
        ObservationPayloadRef::BrowserEvent(value) => codec::id(value.as_uuid()).to_vec(),
        ObservationPayloadRef::External(value) => value.as_bytes().to_vec(),
    }
}

fn authoritative_path_required() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(
            "frame, capture-gap, and browser-event observations require their authoritative persistence path",
        )
        .expect("static timeline error is non-empty"),
    )
}
