use std::{collections::BTreeMap, num::NonZeroU64};

use krometrail_core::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventCursor, BrowserEventId,
    BrowserEventOrdinal, BrowserEventPayload, BrowserEventSelector, BrowserEventSeverity,
    BrowserEventSource, BrowserEventUnavailableRange, BrowserEventUnavailableReason,
    BrowserSourceClock, BrowserSourceTimestamp, CaptureStatusSamples, EventCandidateLimit,
    EventPageLimit, MAX_BROWSER_EVENT_BATCH_BYTES, MAX_BROWSER_EVENT_BATCH_ROWS,
    MAX_CAPTURE_STATUS_SAMPLES, MAX_EVENT_UNAVAILABLE_RANGES, ObservationKind,
    ObservationPayloadRef, ObservedTime, PortFuture, SessionId, SessionRange, SessionTime,
    SourceTime, TargetId, TimelineObservation,
};
use rusqlite::{
    Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params, params_from_iter,
    types::Value as SqlValue,
};

use crate::persistence_error;

use super::{SqliteIndex, codec, ensure_identity, timeline::append_observation_tx};

/// Fixed logical allowance for SQLite record headers and the event's secondary-index entries.
pub(crate) const EVENT_ROW_ALLOWANCE_BYTES: u64 = 256;
pub(crate) const MAX_EVENT_EVICTION_ROWS: usize = 256;
pub(crate) const MAX_EVENT_EVICTION_BYTES: u64 = 1024 * 1024;
pub(crate) const RECOVERY_CHUNK_ROWS: usize = 128;

const EVENT_COLUMNS: &str = "event_id,session_id,target_id,event_ordinal_be,\
 attachment_generation_be,session_time_be,affected_start_time_be,affected_end_time_be,\
 source_clock,source_time_be,source_rounded,observed_time_be,kind,class,severity_rank,\
 compact_priority,payload_json,accounted_bytes_be,retention_sequence";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BrowserEventCandidate {
    pub retention_sequence: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EventRecoveryReport {
    pub timeline_rows_repaired: u64,
    pub usage_rows_repaired: u64,
    pub corrupt_rows_discarded: u64,
    pub orphan_timeline_rows_discarded: u64,
    pub orphan_usage_rows_removed: u64,
}

impl SqliteIndex {
    pub(crate) fn append_browser_event_batch(
        &self,
        batch: BrowserEventBatch,
        budget_bytes: u64,
        managed_usage_before: u64,
    ) -> krometrail_core::Result<()> {
        if batch.events().is_empty()
            || batch.events().len() > MAX_BROWSER_EVENT_BATCH_ROWS
            || serde_json::to_vec(&batch)
                .map_err(|_| persistence_error("could not validate browser event batch"))?
                .len()
                > MAX_BROWSER_EVENT_BATCH_BYTES
        {
            return Err(persistence_error(
                "browser event batch exceeds its bounded contract",
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin browser event persistence"))?;
        let mut inserted_bytes = 0_u64;
        for event in batch.events() {
            event
                .validate()
                .map_err(|_| persistence_error("browser event is invalid"))?;
            inserted_bytes = inserted_bytes
                .checked_add(append_event_tx(&transaction, event)?)
                .ok_or_else(|| persistence_error("browser event batch usage overflow"))?;
        }
        // The global usage snapshot is measured before beginning this short write
        // transaction. Only bounded new rows and bounded evictions can change it here.
        let mut total = managed_usage_before
            .checked_add(inserted_bytes)
            .ok_or_else(|| persistence_error("managed usage overflow"))?;
        if total > budget_bytes {
            let removed = evict_events_tx(
                &transaction,
                None,
                MAX_EVENT_EVICTION_ROWS,
                MAX_EVENT_EVICTION_BYTES,
                Some(total - budget_bytes),
            )?;
            total = total.saturating_sub(removed);
        }
        if total > budget_bytes {
            return Err(krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::BudgetExhausted,
                krometrail_core::NonEmptyText::new(
                    "disk budget cannot retain the browser event batch",
                )
                .expect("static browser event budget error is non-empty"),
            ));
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit browser event persistence"))
    }

    pub(crate) fn oldest_browser_event(
        &self,
    ) -> krometrail_core::Result<Option<BrowserEventCandidate>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT retention_sequence FROM browser_events \
                 ORDER BY retention_sequence,event_id LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| persistence_error("could not select oldest browser event"))?
            .map(|value| {
                u64::try_from(value)
                    .map(|retention_sequence| BrowserEventCandidate { retention_sequence })
                    .map_err(|_| persistence_error("stored retention sequence is malformed"))
            })
            .transpose()
    }

    pub(crate) fn evict_oldest_browser_events(
        &self,
        before_sequence: Option<u64>,
    ) -> krometrail_core::Result<u64> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin browser event eviction"))?;
        let removed = evict_events_tx(
            &transaction,
            before_sequence,
            MAX_EVENT_EVICTION_ROWS,
            MAX_EVENT_EVICTION_BYTES,
            None,
        )?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit browser event eviction"))?;
        Ok(removed)
    }

    pub(crate) fn recover_browser_events(&self) -> krometrail_core::Result<EventRecoveryReport> {
        let mut report = EventRecoveryReport::default();
        let mut after_event_id: Option<Vec<u8>> = None;
        loop {
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin browser event recovery"))?;
            let rows =
                raw_rows_after(&transaction, after_event_id.as_deref(), RECOVERY_CHUNK_ROWS)?;
            if rows.is_empty() {
                transaction
                    .commit()
                    .map_err(|_| persistence_error("could not close browser event recovery"))?;
                break;
            }
            for raw in &rows {
                after_event_id = Some(raw.event_id.clone());
                let scope = raw.decode_scope()?;
                match raw.decode_event() {
                    Ok(event) => {
                        report.timeline_rows_repaired +=
                            u64::from(reconcile_timeline_tx(&transaction, &event)?);
                        report.usage_rows_repaired += u64::from(reconcile_usage_tx(
                            &transaction,
                            &event,
                            raw.accounted_bytes()?,
                        )?);
                    }
                    Err(_) => {
                        remove_event_dependents_tx(&transaction, scope.event_id)?;
                        transaction
                            .execute(
                                "DELETE FROM browser_events WHERE event_id=?1",
                                params![codec::id(scope.event_id.as_uuid()).to_vec()],
                            )
                            .map_err(|_| {
                                persistence_error("could not discard corrupt browser event")
                            })?;
                        record_unavailable_tx(
                            &transaction,
                            scope.session_id,
                            scope.target_id,
                            SessionRange::new(scope.session_time, scope.session_time).map_err(
                                |_| persistence_error("stored browser event time is malformed"),
                            )?,
                            Some(scope.ordinal),
                            Some(scope.ordinal),
                            1,
                            BrowserEventUnavailableReason::CorruptDiscarded,
                        )?;
                        report.corrupt_rows_discarded += 1;
                    }
                }
            }
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit browser event recovery"))?;
            if rows.len() < RECOVERY_CHUNK_ROWS {
                break;
            }
        }
        recover_orphan_timeline(self, &mut report)?;
        recover_orphan_usage(self, &mut report)?;
        Ok(report)
    }
}

impl BrowserEventSource for SqliteIndex {
    fn count_events(
        &self,
        selector: BrowserEventSelector,
    ) -> PortFuture<'_, krometrail_core::Result<u64>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let (filter, values) = selector_filter(&selector, "")?;
            connection
                .query_row(
                    &format!("SELECT count(*) FROM browser_events WHERE {filter}"),
                    params_from_iter(values),
                    |row| row.get::<_, u64>(0),
                )
                .map_err(|_| persistence_error("could not count browser events"))
        })
    }

    fn chronological_events(
        &self,
        selector: BrowserEventSelector,
        cursor: Option<BrowserEventCursor>,
        limit: EventPageLimit,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserEvent>>> {
        Box::pin(async move {
            if cursor
                .as_ref()
                .is_some_and(|cursor| cursor.selector() != &selector)
            {
                return Err(invalid_query(
                    "browser event cursor does not match its selector",
                ));
            }
            let connection = self.connection()?;
            let (mut filter, mut values) = selector_filter(&selector, "")?;
            if let Some(cursor) = cursor {
                filter.push_str(
                    " AND (session_time_be>? OR (session_time_be=? AND event_ordinal_be>?) \
                     OR (session_time_be=? AND event_ordinal_be=? AND event_id>?))",
                );
                let time = codec::u64_blob(cursor.session_time().as_nanos()).to_vec();
                let ordinal = codec::u64_blob(cursor.ordinal().get()).to_vec();
                values.extend([
                    SqlValue::Blob(time.clone()),
                    SqlValue::Blob(time.clone()),
                    SqlValue::Blob(ordinal.clone()),
                    SqlValue::Blob(time),
                    SqlValue::Blob(ordinal),
                    SqlValue::Blob(codec::id(cursor.event_id().as_uuid()).to_vec()),
                ]);
            }
            values.push(SqlValue::Integer(i64::from(limit.get())));
            query_events(
                &connection,
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM browser_events WHERE {filter} \
                     ORDER BY session_time_be,event_ordinal_be,event_id LIMIT ?"
                ),
                values,
            )
        })
    }

    fn priority_candidates(
        &self,
        selector: BrowserEventSelector,
        limit: EventCandidateLimit,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserEvent>>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let (filter, mut values) = selector_filter(&selector, "")?;
            values.push(SqlValue::Integer(i64::from(limit.get())));
            query_events(
                &connection,
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM browser_events WHERE {filter} \
                     ORDER BY compact_priority,session_time_be,event_ordinal_be,event_id LIMIT ?"
                ),
                values,
            )
        })
    }

    fn nearest_candidates(
        &self,
        selector: BrowserEventSelector,
        focus_times: Vec<SessionTime>,
        each_side: u8,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserEvent>>> {
        Box::pin(async move {
            if focus_times.is_empty()
                || focus_times.len() > 16
                || !(1..=2).contains(&each_side)
                || focus_times
                    .iter()
                    .any(|time| !selector.range().contains(*time))
            {
                return Err(invalid_query(
                    "browser event nearest request is out of range",
                ));
            }
            let connection = self.connection()?;
            let mut events = BTreeMap::new();
            for focus in focus_times {
                for (operator, order) in [("<=", "DESC"), (">=", "ASC")] {
                    let (mut filter, mut values) = selector_filter(&selector, "")?;
                    filter.push_str(&format!(" AND session_time_be{operator}?"));
                    values.push(SqlValue::Blob(codec::u64_blob(focus.as_nanos()).to_vec()));
                    values.push(SqlValue::Integer(i64::from(each_side)));
                    for event in query_events(
                        &connection,
                        &format!(
                            "SELECT {EVENT_COLUMNS} FROM browser_events WHERE {filter} \
                             ORDER BY session_time_be {order},event_ordinal_be {order},event_id {order} LIMIT ?"
                        ),
                        values,
                    )? {
                        events.insert(event.id(), event);
                    }
                }
            }
            let mut events = events.into_values().collect::<Vec<_>>();
            events.sort_by_key(|event| (event.session_time(), event.ordinal(), event.id()));
            Ok(events)
        })
    }

    fn unavailable_ranges(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserEventUnavailableRange>>> {
        Box::pin(async move {
            if session_id.as_uuid().is_nil()
                || target_id.as_uuid().is_nil()
                || limit == 0
                || limit > MAX_EVENT_UNAVAILABLE_RANGES
            {
                return Err(invalid_query(
                    "browser event unavailable request is out of range",
                ));
            }
            let connection = self.connection()?;
            let mut statement = connection
                .prepare(
                    "SELECT session_id,target_id,start_time_be,end_time_be,first_ordinal_be,\
                            last_ordinal_be,event_count_be,reason \
                     FROM browser_event_unavailable_ranges \
                     WHERE session_id=?1 AND target_id=?2 AND start_time_be<=?3 AND end_time_be>=?4 \
                     ORDER BY start_time_be,end_time_be,unavailable_id LIMIT ?5",
                )
                .map_err(|_| persistence_error("could not prepare browser event unavailable query"))?;
            let rows = statement
                .query_map(
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                        codec::u64_blob(range.end().as_nanos()).to_vec(),
                        codec::u64_blob(range.start().as_nanos()).to_vec(),
                        i64::from(limit),
                    ],
                    raw_unavailable,
                )
                .map_err(|_| {
                    persistence_error("could not query browser event unavailable ranges")
                })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| persistence_error("could not read browser event unavailable ranges"))?
                .into_iter()
                .map(decode_unavailable)
                .collect()
        })
    }

    fn capture_status_samples(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, krometrail_core::Result<CaptureStatusSamples>> {
        Box::pin(async move {
            if session_id.as_uuid().is_nil()
                || target_id.as_uuid().is_nil()
                || limit == 0
                || limit > MAX_CAPTURE_STATUS_SAMPLES
            {
                return Err(invalid_query(
                    "capture status sample request is out of range",
                ));
            }
            let connection = self.connection()?;
            let base = "session_id=? AND target_id=? AND kind='capture_status_changed'";
            let scope = vec![
                SqlValue::Blob(codec::id(session_id.as_uuid()).to_vec()),
                SqlValue::Blob(codec::id(target_id.as_uuid()).to_vec()),
            ];
            let mut before_values = scope.clone();
            before_values.push(SqlValue::Blob(
                codec::u64_blob(range.start().as_nanos()).to_vec(),
            ));
            let before = query_events(
                &connection,
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM browser_events WHERE {base} AND session_time_be<=? \
                     ORDER BY session_time_be DESC,event_ordinal_be DESC,event_id DESC LIMIT 1"
                ),
                before_values,
            )?
            .into_iter()
            .next();
            let mut range_values = scope;
            range_values.push(SqlValue::Blob(
                codec::u64_blob(range.start().as_nanos()).to_vec(),
            ));
            range_values.push(SqlValue::Blob(
                codec::u64_blob(range.end().as_nanos()).to_vec(),
            ));
            range_values.push(SqlValue::Integer(i64::from(limit)));
            let in_range = query_events(
                &connection,
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM browser_events WHERE {base} \
                     AND session_time_be>=? AND session_time_be<=? \
                     ORDER BY session_time_be,event_ordinal_be,event_id LIMIT ?"
                ),
                range_values,
            )?;
            CaptureStatusSamples::new(before, in_range)
                .map_err(|_| persistence_error("stored capture status samples are invalid"))
        })
    }
}

fn append_event_tx(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
) -> krometrail_core::Result<u64> {
    ensure_identity(transaction, event.session_id(), event.target_id())?;
    let conflicts = conflicting_rows(transaction, event)?;
    if !conflicts.is_empty() {
        if conflicts.len() == 1 && conflicts[0].decode_event()? == *event {
            ensure_event_dependents_tx(transaction, event, conflicts[0].accounted_bytes()?)?;
            return Ok(0);
        }
        return Err(persistence_error(
            "browser event identity conflicts with its durable value",
        ));
    }

    let payload_json = serde_json::to_string(event.payload())
        .map_err(|_| persistence_error("could not encode browser event payload"))?;
    let accounted_bytes = accounted_bytes(event, payload_json.len())?;
    let retention_sequence = allocate_retention_sequence(transaction)?;
    let affected = event.affected_range();
    let (source_clock, source_time, source_rounded) =
        event.source_time().map_or((None, None, 0_i64), |source| {
            (
                Some(source_clock_name(source.clock())),
                Some(codec::i128_blob(source.time().as_nanos()).to_vec()),
                i64::from(source.rounded()),
            )
        });
    transaction
        .execute(
            "INSERT INTO browser_events(\
                event_id,session_id,target_id,event_ordinal_be,attachment_generation_be,\
                session_time_be,affected_start_time_be,affected_end_time_be,source_clock,\
                source_time_be,source_rounded,observed_time_be,kind,class,severity_rank,\
                compact_priority,payload_json,accounted_bytes_be,retention_sequence\
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![
                codec::id(event.id().as_uuid()).to_vec(),
                codec::id(event.session_id().as_uuid()).to_vec(),
                codec::id(event.target_id().as_uuid()).to_vec(),
                codec::u64_blob(event.ordinal().get()).to_vec(),
                codec::u64_blob(event.attachment_generation()).to_vec(),
                codec::u64_blob(event.session_time().as_nanos()).to_vec(),
                codec::u64_blob(affected.start().as_nanos()).to_vec(),
                codec::u64_blob(affected.end().as_nanos()).to_vec(),
                source_clock,
                source_time,
                source_rounded,
                codec::u64_blob(event.observed_time().as_nanos()).to_vec(),
                event.kind().as_str(),
                class_name(event.class()),
                severity_rank(event.severity()),
                i64::from(event.compact_priority()),
                payload_json,
                codec::u64_blob(accounted_bytes).to_vec(),
                i64::try_from(retention_sequence)
                    .map_err(|_| persistence_error("retention sequence exceeds SQLite limits"))?,
            ],
        )
        .map_err(|_| persistence_error("could not persist browser event"))?;
    ensure_event_dependents_tx(transaction, event, accounted_bytes)?;
    Ok(accounted_bytes)
}

fn conflicting_rows(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
) -> krometrail_core::Result<Vec<RawEvent>> {
    let mut statement = transaction
        .prepare(&format!(
            "SELECT {EVENT_COLUMNS} FROM browser_events WHERE event_id=?1 OR \
             (session_id=?2 AND target_id=?3 AND event_ordinal_be=?4) ORDER BY event_id"
        ))
        .map_err(|_| persistence_error("could not prepare browser event replay validation"))?;
    let rows = statement
        .query_map(
            params![
                codec::id(event.id().as_uuid()).to_vec(),
                codec::id(event.session_id().as_uuid()).to_vec(),
                codec::id(event.target_id().as_uuid()).to_vec(),
                codec::u64_blob(event.ordinal().get()).to_vec(),
            ],
            raw_event,
        )
        .map_err(|_| persistence_error("could not validate browser event replay"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read browser event replay"))
}

fn ensure_event_dependents_tx(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
    accounted_bytes: u64,
) -> krometrail_core::Result<()> {
    ensure_timeline_tx(transaction, event)?;
    ensure_usage_tx(transaction, event, accounted_bytes)?;
    Ok(())
}

fn ensure_timeline_tx(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
) -> krometrail_core::Result<bool> {
    let key = codec::id(event.id().as_uuid()).to_vec();
    let exists = transaction
        .query_row(
            "SELECT 1 FROM timeline_observations WHERE kind='browser_event' AND payload_sort_key=?1",
            params![&key],
            |_| Ok(()),
        )
        .optional()
        .map_err(|_| persistence_error("could not validate browser event timeline reference"))?
        .is_some();
    if exists {
        validate_timeline_reference(transaction, event)?;
        return Ok(false);
    }
    let observation = timeline_observation(event)?;
    append_observation_tx(transaction, &observation, None)?;
    Ok(true)
}

fn ensure_usage_tx(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
    accounted_bytes: u64,
) -> krometrail_core::Result<bool> {
    let key = codec::id(event.id().as_uuid()).to_vec();
    let existing: Option<(Option<Vec<u8>>, Vec<u8>)> = transaction
        .query_row(
            "SELECT session_id,byte_len_be FROM usage WHERE class='browser_event' AND object_key=?1",
            params![&key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| persistence_error("could not validate browser event usage"))?;
    let expected_session = codec::id(event.session_id().as_uuid()).to_vec();
    let expected_bytes = codec::u64_blob(accounted_bytes).to_vec();
    if let Some((session, bytes)) = existing {
        if session.as_deref() == Some(expected_session.as_slice()) && bytes == expected_bytes {
            return Ok(false);
        }
        return Err(persistence_error(
            "browser event usage disagrees with its row",
        ));
    }
    transaction
        .execute(
            "INSERT INTO usage(class,object_key,session_id,byte_len_be) \
             VALUES ('browser_event',?1,?2,?3)",
            params![key, expected_session, expected_bytes],
        )
        .map_err(|_| persistence_error("could not persist browser event usage"))?;
    Ok(true)
}

fn reconcile_timeline_tx(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
) -> krometrail_core::Result<bool> {
    let key = codec::id(event.id().as_uuid()).to_vec();
    let exists = transaction
        .query_row(
            "SELECT 1 FROM timeline_observations WHERE kind='browser_event' AND payload_sort_key=?1",
            params![&key],
            |_| Ok(()),
        )
        .optional()
        .map_err(|_| persistence_error("could not inspect browser event timeline recovery"))?
        .is_some();
    if exists && validate_timeline_reference(transaction, event).is_ok() {
        return Ok(false);
    }
    if exists {
        transaction
            .execute(
                "DELETE FROM timeline_observations WHERE kind='browser_event' AND payload_sort_key=?1",
                params![&key],
            )
            .map_err(|_| persistence_error("could not replace browser event timeline reference"))?;
    }
    append_observation_tx(transaction, &timeline_observation(event)?, None)?;
    Ok(true)
}

fn reconcile_usage_tx(
    transaction: &Transaction<'_>,
    event: &BrowserEvent,
    accounted_bytes: u64,
) -> krometrail_core::Result<bool> {
    let key = codec::id(event.id().as_uuid()).to_vec();
    let expected_session = codec::id(event.session_id().as_uuid()).to_vec();
    let expected_bytes = codec::u64_blob(accounted_bytes).to_vec();
    let existing: Option<(Option<Vec<u8>>, Vec<u8>)> = transaction
        .query_row(
            "SELECT session_id,byte_len_be FROM usage WHERE class='browser_event' AND object_key=?1",
            params![&key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| persistence_error("could not inspect browser event usage recovery"))?;
    if existing.as_ref().is_some_and(|(session, bytes)| {
        session.as_deref() == Some(expected_session.as_slice()) && bytes == &expected_bytes
    }) {
        return Ok(false);
    }
    transaction
        .execute(
            "INSERT INTO usage(class,object_key,session_id,byte_len_be) \
             VALUES ('browser_event',?1,?2,?3) \
             ON CONFLICT(class,object_key) DO UPDATE SET \
                session_id=excluded.session_id,byte_len_be=excluded.byte_len_be",
            params![key, expected_session, expected_bytes],
        )
        .map_err(|_| persistence_error("could not reconcile browser event usage"))?;
    Ok(true)
}

fn validate_timeline_reference(
    connection: &Connection,
    event: &BrowserEvent,
) -> krometrail_core::Result<()> {
    let payload = serde_json::to_string(&ObservationPayloadRef::BrowserEvent(event.id()))
        .map_err(|_| persistence_error("could not encode browser event timeline reference"))?;
    let expected_source = event
        .source_time()
        .map(|source| codec::i128_blob(source.time().as_nanos()).to_vec());
    let matches: u64 = connection
        .query_row(
            "SELECT count(*) FROM timeline_observations WHERE kind='browser_event' \
             AND payload_sort_key=?1 AND session_id=?2 AND target_id=?3 AND session_time_be=?4 \
             AND source_time_be IS ?5 AND observed_time_be=?6 AND payload_json=?7",
            params![
                codec::id(event.id().as_uuid()).to_vec(),
                codec::id(event.session_id().as_uuid()).to_vec(),
                codec::id(event.target_id().as_uuid()).to_vec(),
                codec::u64_blob(event.session_time().as_nanos()).to_vec(),
                expected_source,
                codec::u64_blob(event.observed_time().as_nanos()).to_vec(),
                payload,
            ],
            |row| row.get(0),
        )
        .map_err(|_| persistence_error("could not validate browser event timeline reference"))?;
    if matches != 1 {
        return Err(persistence_error(
            "browser event timeline reference disagrees with its row",
        ));
    }
    Ok(())
}

fn timeline_observation(event: &BrowserEvent) -> krometrail_core::Result<TimelineObservation> {
    TimelineObservation::new(
        event.session_id(),
        event.target_id(),
        event.session_time(),
        event.source_time().map(|source| source.time()),
        event.observed_time(),
        ObservationKind::BrowserEvent,
        ObservationPayloadRef::BrowserEvent(event.id()),
    )
    .map_err(|_| persistence_error("browser event timeline projection is invalid"))
}

fn allocate_retention_sequence(transaction: &Transaction<'_>) -> krometrail_core::Result<u64> {
    let next: i64 = transaction
        .query_row(
            "SELECT next_value FROM retention_sequence WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| persistence_error("could not allocate browser event retention sequence"))?;
    transaction
        .execute(
            "UPDATE retention_sequence SET next_value=next_value+1 WHERE singleton=1",
            [],
        )
        .map_err(|_| persistence_error("could not advance browser event retention sequence"))?;
    u64::try_from(next).map_err(|_| persistence_error("stored retention sequence is malformed"))
}

fn evict_events_tx(
    transaction: &Transaction<'_>,
    before_sequence: Option<u64>,
    max_rows: usize,
    max_bytes: u64,
    required_bytes: Option<u64>,
) -> krometrail_core::Result<u64> {
    let before = before_sequence
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| persistence_error("retention sequence exceeds SQLite limits"))
        })
        .transpose()?;
    let mut statement = transaction
        .prepare(&format!(
            "SELECT {EVENT_COLUMNS} FROM browser_events \
             WHERE (?1 IS NULL OR retention_sequence<?1) \
             ORDER BY retention_sequence,event_id LIMIT ?2"
        ))
        .map_err(|_| persistence_error("could not prepare browser event eviction"))?;
    let rows = statement
        .query_map(
            params![before, i64::try_from(max_rows).unwrap_or(i64::MAX)],
            raw_event,
        )
        .map_err(|_| persistence_error("could not query browser event eviction"))?;
    let candidates = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read browser event eviction"))?;
    drop(statement);
    let mut removed = 0_u64;
    for raw in candidates {
        let accounted = raw.accounted_bytes()?;
        if removed > 0 && removed.saturating_add(accounted) > max_bytes {
            break;
        }
        let scope = raw.decode_scope()?;
        remove_event_dependents_tx(transaction, scope.event_id)?;
        transaction
            .execute(
                "DELETE FROM browser_events WHERE event_id=?1",
                params![codec::id(scope.event_id.as_uuid()).to_vec()],
            )
            .map_err(|_| persistence_error("could not evict browser event"))?;
        record_unavailable_tx(
            transaction,
            scope.session_id,
            scope.target_id,
            SessionRange::new(scope.session_time, scope.session_time)
                .map_err(|_| persistence_error("stored browser event time is malformed"))?,
            Some(scope.ordinal),
            Some(scope.ordinal),
            1,
            BrowserEventUnavailableReason::RetentionEvicted,
        )?;
        removed = removed
            .checked_add(accounted)
            .ok_or_else(|| persistence_error("browser event eviction byte count overflow"))?;
        if required_bytes.is_some_and(|required| removed >= required) {
            break;
        }
    }
    Ok(removed)
}

fn remove_event_dependents_tx(
    transaction: &Transaction<'_>,
    event_id: BrowserEventId,
) -> krometrail_core::Result<()> {
    let key = codec::id(event_id.as_uuid()).to_vec();
    transaction
        .execute(
            "DELETE FROM timeline_observations WHERE kind='browser_event' AND payload_sort_key=?1",
            params![&key],
        )
        .map_err(|_| persistence_error("could not remove browser event timeline reference"))?;
    transaction
        .execute(
            "DELETE FROM usage WHERE class='browser_event' AND object_key=?1",
            params![key],
        )
        .map_err(|_| persistence_error("could not remove browser event usage"))?;
    Ok(())
}

struct UnavailableTail {
    id: i64,
    start: Vec<u8>,
    end: Vec<u8>,
    first_ordinal: Option<Vec<u8>>,
    last_ordinal: Option<Vec<u8>>,
    event_count: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
fn record_unavailable_tx(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    first_ordinal: Option<BrowserEventOrdinal>,
    last_ordinal: Option<BrowserEventOrdinal>,
    count: u64,
    reason: BrowserEventUnavailableReason,
) -> krometrail_core::Result<()> {
    let existing: Option<UnavailableTail> = transaction
        .query_row(
            "SELECT unavailable_id,start_time_be,end_time_be,first_ordinal_be,last_ordinal_be,event_count_be \
             FROM browser_event_unavailable_ranges WHERE session_id=?1 AND target_id=?2 AND reason=?3 \
             ORDER BY end_time_be DESC,unavailable_id DESC LIMIT 1",
            params![
                codec::id(session_id.as_uuid()).to_vec(),
                codec::id(target_id.as_uuid()).to_vec(),
                reason.as_str(),
            ],
            |row| {
                Ok(UnavailableTail {
                    id: row.get(0)?,
                    start: row.get(1)?,
                    end: row.get(2)?,
                    first_ordinal: row.get(3)?,
                    last_ordinal: row.get(4)?,
                    event_count: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|_| persistence_error("could not query browser event unavailable range"))?;
    if let Some(existing) = existing {
        let stored_start = codec::decode_u64(&existing.start)?;
        let stored_end = codec::decode_u64(&existing.end)?;
        let stored_first = existing
            .first_ordinal
            .as_deref()
            .map(codec::decode_u64)
            .transpose()?;
        let stored_last = existing
            .last_ordinal
            .as_deref()
            .map(codec::decode_u64)
            .transpose()?;
        let ordinal_contiguous = stored_last
            .zip(first_ordinal.map(BrowserEventOrdinal::get))
            .is_some_and(|(old, new)| old.saturating_add(1) >= new);
        let time_contiguous = stored_end.saturating_add(1) >= range.start().as_nanos();
        if (ordinal_contiguous || time_contiguous)
            && stored_first.is_some() == first_ordinal.is_some()
        {
            let merged_count = codec::decode_u64(&existing.event_count)?
                .checked_add(count)
                .ok_or_else(|| persistence_error("browser event unavailable count overflow"))?;
            transaction
                .execute(
                    "UPDATE browser_event_unavailable_ranges SET start_time_be=?1,end_time_be=?2,\
                     first_ordinal_be=?3,last_ordinal_be=?4,event_count_be=?5 WHERE unavailable_id=?6",
                    params![
                        codec::u64_blob(stored_start.min(range.start().as_nanos())).to_vec(),
                        codec::u64_blob(stored_end.max(range.end().as_nanos())).to_vec(),
                        stored_first
                            .zip(first_ordinal.map(BrowserEventOrdinal::get))
                            .map(|(old, new)| codec::u64_blob(old.min(new)).to_vec()),
                        stored_last
                            .zip(last_ordinal.map(BrowserEventOrdinal::get))
                            .map(|(old, new)| codec::u64_blob(old.max(new)).to_vec()),
                        codec::u64_blob(merged_count).to_vec(),
                        existing.id,
                    ],
                )
                .map_err(|_| persistence_error("could not merge browser event unavailable range"))?;
            return Ok(());
        }
    }
    let count = NonZeroU64::new(count)
        .ok_or_else(|| persistence_error("browser event unavailable count must be non-zero"))?;
    BrowserEventUnavailableRange::new(
        session_id,
        target_id,
        range,
        first_ordinal,
        last_ordinal,
        count,
        reason,
    )
    .map_err(|_| persistence_error("browser event unavailable range is invalid"))?;
    transaction
        .execute(
            "INSERT INTO browser_event_unavailable_ranges(\
                session_id,target_id,start_time_be,end_time_be,first_ordinal_be,last_ordinal_be,\
                event_count_be,reason) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                codec::id(session_id.as_uuid()).to_vec(),
                codec::id(target_id.as_uuid()).to_vec(),
                codec::u64_blob(range.start().as_nanos()).to_vec(),
                codec::u64_blob(range.end().as_nanos()).to_vec(),
                first_ordinal.map(|value| codec::u64_blob(value.get()).to_vec()),
                last_ordinal.map(|value| codec::u64_blob(value.get()).to_vec()),
                codec::u64_blob(count.get()).to_vec(),
                reason.as_str(),
            ],
        )
        .map_err(|_| persistence_error("could not record browser event unavailable range"))?;
    Ok(())
}

fn selector_filter(
    selector: &BrowserEventSelector,
    prefix: &str,
) -> krometrail_core::Result<(String, Vec<SqlValue>)> {
    let mut filter = format!(
        "{prefix}session_id=? AND {prefix}target_id=? AND {prefix}session_time_be>=? \
         AND {prefix}session_time_be<=? AND {prefix}severity_rank>=?"
    );
    let mut values = vec![
        SqlValue::Blob(codec::id(selector.session_id().as_uuid()).to_vec()),
        SqlValue::Blob(codec::id(selector.target_id().as_uuid()).to_vec()),
        SqlValue::Blob(codec::u64_blob(selector.range().start().as_nanos()).to_vec()),
        SqlValue::Blob(codec::u64_blob(selector.range().end().as_nanos()).to_vec()),
        SqlValue::Integer(severity_rank(selector.minimum_severity())),
    ];
    if !selector.classes().is_empty() {
        filter.push_str(&format!(
            " AND {prefix}class IN ({})",
            std::iter::repeat_n("?", selector.classes().len())
                .collect::<Vec<_>>()
                .join(",")
        ));
        values.extend(
            selector
                .classes()
                .iter()
                .map(|class| SqlValue::Text(class_name(*class).to_owned())),
        );
    }
    Ok((filter, values))
}

fn query_events(
    connection: &Connection,
    sql: &str,
    values: Vec<SqlValue>,
) -> krometrail_core::Result<Vec<BrowserEvent>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| persistence_error("could not prepare browser event query"))?;
    let rows = statement
        .query_map(params_from_iter(values), raw_event)
        .map_err(|_| persistence_error("could not query browser events"))?;
    let raw = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read browser events"))?;
    let mut events = Vec::with_capacity(raw.len());
    for raw in raw {
        let event = raw.decode_event()?;
        validate_timeline_reference(connection, &event)?;
        events.push(event);
    }
    Ok(events)
}

#[derive(Clone)]
struct RawEvent {
    event_id: Vec<u8>,
    session_id: Vec<u8>,
    target_id: Vec<u8>,
    ordinal: Vec<u8>,
    attachment_generation: Vec<u8>,
    session_time: Vec<u8>,
    affected_start: Vec<u8>,
    affected_end: Vec<u8>,
    source_clock: Option<String>,
    source_time: Option<Vec<u8>>,
    source_rounded: i64,
    observed_time: Vec<u8>,
    kind: String,
    class: String,
    severity_rank: i64,
    compact_priority: i64,
    payload_json: String,
    accounted_bytes: Vec<u8>,
    retention_sequence: i64,
}

struct EventScope {
    event_id: BrowserEventId,
    session_id: SessionId,
    target_id: TargetId,
    ordinal: BrowserEventOrdinal,
    session_time: SessionTime,
}

impl RawEvent {
    fn decode_scope(&self) -> krometrail_core::Result<EventScope> {
        let event_id = BrowserEventId::from_uuid(codec::decode_id(&self.event_id)?);
        let session_id = SessionId::from_uuid(codec::decode_id(&self.session_id)?);
        let target_id = TargetId::from_uuid(codec::decode_id(&self.target_id)?);
        let ordinal = BrowserEventOrdinal::new(codec::decode_u64(&self.ordinal)?)
            .map_err(|_| persistence_error("stored browser event ordinal is malformed"))?;
        let session_time = SessionTime::from_nanos(codec::decode_u64(&self.session_time)?);
        let affected_start = codec::decode_u64(&self.affected_start)?;
        let affected_end = codec::decode_u64(&self.affected_end)?;
        if event_id.as_uuid().is_nil()
            || session_id.as_uuid().is_nil()
            || target_id.as_uuid().is_nil()
            || codec::decode_u64(&self.attachment_generation)? == 0
            || codec::decode_u64(&self.observed_time)? < session_time.as_nanos()
            || affected_start > affected_end
            || session_time.as_nanos() < affected_start
            || session_time.as_nanos() > affected_end
        {
            return Err(persistence_error(
                "stored browser event identity or time is malformed",
            ));
        }
        Ok(EventScope {
            event_id,
            session_id,
            target_id,
            ordinal,
            session_time,
        })
    }

    fn decode_event(&self) -> krometrail_core::Result<BrowserEvent> {
        let scope = self.decode_scope()?;
        let source_time = match (&self.source_clock, &self.source_time) {
            (None, None) if self.source_rounded == 0 => None,
            (Some(clock), Some(time)) => Some(
                BrowserSourceTimestamp::new(
                    decode_source_clock(clock)?,
                    SourceTime::from_nanos(codec::decode_i128(time)?),
                    decode_bool(self.source_rounded)?,
                )
                .map_err(|_| persistence_error("stored browser source timestamp is malformed"))?,
            ),
            _ => {
                return Err(persistence_error(
                    "stored browser source timestamp is malformed",
                ));
            }
        };
        let payload: BrowserEventPayload = serde_json::from_str(&self.payload_json)
            .map_err(|_| persistence_error("stored browser event payload is malformed"))?;
        let attachment = codec::decode_u64(&self.attachment_generation)?;
        let event = BrowserEvent::new(
            scope.event_id,
            scope.session_id,
            scope.target_id,
            attachment,
            scope.ordinal,
            scope.session_time,
            source_time,
            ObservedTime::from_nanos(codec::decode_u64(&self.observed_time)?),
            decode_severity(self.severity_rank)?,
            payload,
        )
        .map_err(|_| persistence_error("stored browser event is invalid"))?;
        let affected = event.affected_range();
        if self.kind != event.kind().as_str()
            || self.class != class_name(event.class())
            || self.compact_priority != i64::from(event.compact_priority())
            || codec::decode_u64(&self.affected_start)? != affected.start().as_nanos()
            || codec::decode_u64(&self.affected_end)? != affected.end().as_nanos()
            || self.accounted_bytes()? != accounted_bytes(&event, self.payload_json.len())?
        {
            return Err(persistence_error(
                "stored browser event projection is inconsistent",
            ));
        }
        self.retention_sequence_u64()?;
        Ok(event)
    }

    fn accounted_bytes(&self) -> krometrail_core::Result<u64> {
        codec::decode_u64(&self.accounted_bytes)
    }

    fn retention_sequence_u64(&self) -> krometrail_core::Result<u64> {
        u64::try_from(self.retention_sequence)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| persistence_error("stored retention sequence is malformed"))
    }
}

fn raw_event(row: &Row<'_>) -> rusqlite::Result<RawEvent> {
    Ok(RawEvent {
        event_id: row.get(0)?,
        session_id: row.get(1)?,
        target_id: row.get(2)?,
        ordinal: row.get(3)?,
        attachment_generation: row.get(4)?,
        session_time: row.get(5)?,
        affected_start: row.get(6)?,
        affected_end: row.get(7)?,
        source_clock: row.get(8)?,
        source_time: row.get(9)?,
        source_rounded: row.get(10)?,
        observed_time: row.get(11)?,
        kind: row.get(12)?,
        class: row.get(13)?,
        severity_rank: row.get(14)?,
        compact_priority: row.get(15)?,
        payload_json: row.get(16)?,
        accounted_bytes: row.get(17)?,
        retention_sequence: row.get(18)?,
    })
}

fn raw_rows_after(
    connection: &Connection,
    after_event_id: Option<&[u8]>,
    limit: usize,
) -> krometrail_core::Result<Vec<RawEvent>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {EVENT_COLUMNS} FROM browser_events \
             WHERE (?1 IS NULL OR event_id>?1) ORDER BY event_id LIMIT ?2"
        ))
        .map_err(|_| persistence_error("could not prepare browser event recovery scan"))?;
    let rows = statement
        .query_map(
            params![after_event_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            raw_event,
        )
        .map_err(|_| persistence_error("could not query browser event recovery scan"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read browser event recovery scan"))
}

fn accounted_bytes(event: &BrowserEvent, payload_bytes: usize) -> krometrail_core::Result<u64> {
    let (source_clock_bytes, source_time_bytes) =
        event.source_time().map_or((0_usize, 0_usize), |source| {
            (source_clock_name(source.clock()).len(), 16)
        });
    // Three UUIDs, seven fixed-width u64 blobs, four integer projections,
    // optional i128 source time, and the exact UTF-8 projection text.
    let projection = 16_usize * 3
        + 8 * 7
        + 8 * 4
        + source_time_bytes
        + event.kind().as_str().len()
        + class_name(event.class()).len()
        + source_clock_bytes;
    u64::try_from(payload_bytes)
        .ok()
        .and_then(|value| value.checked_add(projection as u64))
        .and_then(|value| value.checked_add(EVENT_ROW_ALLOWANCE_BYTES))
        .ok_or_else(|| persistence_error("browser event accounted bytes overflow"))
}

fn class_name(class: BrowserEventClass) -> &'static str {
    match class {
        BrowserEventClass::Console => "console",
        BrowserEventClass::Exception => "exception",
        BrowserEventClass::Network => "network",
        BrowserEventClass::Navigation => "navigation",
        BrowserEventClass::Lifecycle => "lifecycle",
        BrowserEventClass::Target => "target",
        BrowserEventClass::Dialog => "dialog",
        BrowserEventClass::Capture => "capture",
        BrowserEventClass::Operational => "operational",
    }
}

fn severity_rank(severity: BrowserEventSeverity) -> i64 {
    match severity {
        BrowserEventSeverity::Debug => 0,
        BrowserEventSeverity::Info => 1,
        BrowserEventSeverity::Warning => 2,
        BrowserEventSeverity::Error => 3,
    }
}

fn decode_severity(value: i64) -> krometrail_core::Result<BrowserEventSeverity> {
    match value {
        0 => Ok(BrowserEventSeverity::Debug),
        1 => Ok(BrowserEventSeverity::Info),
        2 => Ok(BrowserEventSeverity::Warning),
        3 => Ok(BrowserEventSeverity::Error),
        _ => Err(persistence_error(
            "stored browser event severity is malformed",
        )),
    }
}

fn source_clock_name(clock: BrowserSourceClock) -> &'static str {
    match clock {
        BrowserSourceClock::CdpMonotonic => "cdp_monotonic",
        BrowserSourceClock::UnixEpoch => "unix_epoch",
    }
}

fn decode_source_clock(value: &str) -> krometrail_core::Result<BrowserSourceClock> {
    match value {
        "cdp_monotonic" => Ok(BrowserSourceClock::CdpMonotonic),
        "unix_epoch" => Ok(BrowserSourceClock::UnixEpoch),
        _ => Err(persistence_error(
            "stored browser source clock is malformed",
        )),
    }
}

fn decode_bool(value: i64) -> krometrail_core::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(persistence_error(
            "stored browser event boolean is malformed",
        )),
    }
}

fn raw_unavailable(row: &Row<'_>) -> rusqlite::Result<RawUnavailable> {
    Ok(RawUnavailable {
        session: row.get(0)?,
        target: row.get(1)?,
        start: row.get(2)?,
        end: row.get(3)?,
        first: row.get(4)?,
        last: row.get(5)?,
        count: row.get(6)?,
        reason: row.get(7)?,
    })
}

struct RawUnavailable {
    session: Vec<u8>,
    target: Vec<u8>,
    start: Vec<u8>,
    end: Vec<u8>,
    first: Option<Vec<u8>>,
    last: Option<Vec<u8>>,
    count: Vec<u8>,
    reason: String,
}

fn decode_unavailable(
    raw: RawUnavailable,
) -> krometrail_core::Result<BrowserEventUnavailableRange> {
    BrowserEventUnavailableRange::new(
        SessionId::from_uuid(codec::decode_id(&raw.session)?),
        TargetId::from_uuid(codec::decode_id(&raw.target)?),
        SessionRange::new(
            SessionTime::from_nanos(codec::decode_u64(&raw.start)?),
            SessionTime::from_nanos(codec::decode_u64(&raw.end)?),
        )
        .map_err(|_| persistence_error("stored browser event unavailable range is malformed"))?,
        raw.first
            .as_deref()
            .map(codec::decode_u64)
            .transpose()?
            .map(BrowserEventOrdinal::new)
            .transpose()
            .map_err(|_| {
                persistence_error("stored browser event unavailable ordinal is malformed")
            })?,
        raw.last
            .as_deref()
            .map(codec::decode_u64)
            .transpose()?
            .map(BrowserEventOrdinal::new)
            .transpose()
            .map_err(|_| {
                persistence_error("stored browser event unavailable ordinal is malformed")
            })?,
        NonZeroU64::new(codec::decode_u64(&raw.count)?).ok_or_else(|| {
            persistence_error("stored browser event unavailable count is malformed")
        })?,
        BrowserEventUnavailableReason::from_stable_name(&raw.reason).ok_or_else(|| {
            persistence_error("stored browser event unavailable reason is malformed")
        })?,
    )
    .map_err(|_| persistence_error("stored browser event unavailable range is invalid"))
}

fn recover_orphan_timeline(
    index: &SqliteIndex,
    report: &mut EventRecoveryReport,
) -> krometrail_core::Result<()> {
    loop {
        let mut connection = index.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin orphan event timeline recovery"))?;
        let mut statement = transaction
            .prepare(
                "SELECT observation_id,session_id,target_id,session_time_be,payload_json,payload_sort_key \
                 FROM timeline_observations t WHERE kind='browser_event' \
                   AND NOT EXISTS (SELECT 1 FROM browser_events b WHERE b.event_id=t.payload_sort_key) \
                 ORDER BY observation_id LIMIT ?1",
            )
            .map_err(|_| persistence_error("could not prepare orphan event timeline recovery"))?;
        let rows = statement
            .query_map(
                params![i64::try_from(RECOVERY_CHUNK_ROWS).unwrap()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                    ))
                },
            )
            .map_err(|_| persistence_error("could not query orphan event timeline recovery"))?;
        let rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read orphan event timeline recovery"))?;
        drop(statement);
        for (id, session, target, time, _payload, sort_key) in &rows {
            let session_id = SessionId::from_uuid(codec::decode_id(session)?);
            let target_id = TargetId::from_uuid(codec::decode_id(target)?);
            let event_id = BrowserEventId::from_uuid(codec::decode_id(sort_key)?);
            let session_time = SessionTime::from_nanos(codec::decode_u64(time)?);
            if session_id.as_uuid().is_nil()
                || target_id.as_uuid().is_nil()
                || event_id.as_uuid().is_nil()
            {
                return Err(persistence_error(
                    "stored orphan event timeline identity is malformed",
                ));
            }
            record_unavailable_tx(
                &transaction,
                session_id,
                target_id,
                SessionRange::new(session_time, session_time).map_err(|_| {
                    persistence_error("stored orphan event timeline time is malformed")
                })?,
                None,
                None,
                1,
                BrowserEventUnavailableReason::CorruptDiscarded,
            )?;
            transaction
                .execute(
                    "DELETE FROM timeline_observations WHERE observation_id=?1",
                    params![id],
                )
                .map_err(|_| {
                    persistence_error("could not discard orphan event timeline reference")
                })?;
            report.orphan_timeline_rows_discarded += 1;
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit orphan event timeline recovery"))?;
        if rows.len() < RECOVERY_CHUNK_ROWS {
            break;
        }
    }
    Ok(())
}

fn recover_orphan_usage(
    index: &SqliteIndex,
    report: &mut EventRecoveryReport,
) -> krometrail_core::Result<()> {
    loop {
        let mut connection = index.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin orphan event usage recovery"))?;
        let mut statement = transaction
            .prepare(
                "SELECT object_key FROM usage u WHERE class='browser_event' \
                   AND NOT EXISTS (SELECT 1 FROM browser_events b WHERE b.event_id=u.object_key) \
                 ORDER BY object_key LIMIT ?1",
            )
            .map_err(|_| persistence_error("could not prepare orphan event usage recovery"))?;
        let rows = statement
            .query_map(
                params![i64::try_from(RECOVERY_CHUNK_ROWS).unwrap()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(|_| persistence_error("could not query orphan event usage recovery"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read orphan event usage recovery"))?;
        drop(statement);
        for key in &rows {
            transaction
                .execute(
                    "DELETE FROM usage WHERE class='browser_event' AND object_key=?1",
                    params![key],
                )
                .map_err(|_| persistence_error("could not discard orphan event usage"))?;
            report.orphan_usage_rows_removed += 1;
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit orphan event usage recovery"))?;
        if rows.len() < RECOVERY_CHUNK_ROWS {
            break;
        }
    }
    Ok(())
}

fn invalid_query(message: &'static str) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(
        krometrail_core::ErrorCode::InvalidInput,
        krometrail_core::NonEmptyText::new(message)
            .expect("static browser event query error is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn class_and_severity_projections_are_exhaustive() {
        let classes = BrowserEventClass::ALL
            .iter()
            .map(|class| class_name(*class))
            .collect::<BTreeSet<_>>();
        assert_eq!(classes.len(), BrowserEventClass::ALL.len());
        for (rank, severity) in BrowserEventSeverity::ALL.iter().enumerate() {
            assert_eq!(decode_severity(rank as i64).unwrap(), *severity);
        }
    }
}
