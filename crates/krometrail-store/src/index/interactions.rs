use std::collections::BTreeSet;

use krometrail_core::{
    BrowserOperationKind, InteractionAnchor, InteractionAnchorSource, InteractionId,
    InteractionRecord, InteractionRecordSource, InteractionTiming, NavigationId, ObservationKind,
    ObservationPayloadRef, ObservedTime, PortFuture, SessionId, SessionTime, TargetId,
    TimelineObservation,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use super::{SqliteIndex, codec, ensure_identity, timeline::append_observation_tx};
use crate::persistence_error;

pub(crate) fn append_operation_evidence_tx(
    transaction: &Transaction<'_>,
    anchor: &InteractionAnchor,
    record: Option<&InteractionRecord>,
    persisted_at: ObservedTime,
    navigation_id: Option<NavigationId>,
) -> krometrail_core::Result<()> {
    if let Some(record) = record {
        let record_anchor = record
            .anchor()
            .map_err(|_| persistence_error("interaction record cannot form a durable anchor"))?;
        if &record_anchor != anchor {
            return Err(persistence_error(
                "interaction record does not match its durable anchor",
            ));
        }
    }
    ensure_identity(transaction, anchor.session_id, anchor.target_id)?;
    let record_json = record
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| persistence_error("could not encode interaction record"))?;
    let key = codec::id(anchor.interaction_id.as_uuid()).to_vec();
    let existing = read_raw_by_id(transaction, &key)?;
    if let Some(existing) = existing {
        let (stored_anchor, stored_record) = decode_interaction(existing)?;
        if stored_anchor != *anchor || stored_record.as_ref() != record {
            return Err(persistence_error(
                "interaction identity conflicts with durable evidence",
            ));
        }
    } else {
        transaction
            .execute(
                "INSERT INTO interactions(\
                    interaction_id, session_id, target_id, operation, started_time_be,\
                    dispatched_time_be, completed_time_be, observed_time_be, record_json\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    key,
                    codec::id(anchor.session_id.as_uuid()).to_vec(),
                    codec::id(anchor.target_id.as_uuid()).to_vec(),
                    anchor.operation.stable_name(),
                    codec::u64_blob(anchor.timing.started_at.as_nanos()).to_vec(),
                    codec::u64_blob(anchor.timing.dispatched_at.as_nanos()).to_vec(),
                    codec::u64_blob(anchor.timing.completed_at.as_nanos()).to_vec(),
                    anchor
                        .timing
                        .observed_at
                        .map(|value| codec::u64_blob(value.as_nanos()).to_vec()),
                    record_json,
                ],
            )
            .map_err(|_| persistence_error("could not persist interaction evidence"))?;
    }

    let mut boundary_times = BTreeSet::from([
        anchor.timing.started_at,
        anchor.timing.dispatched_at,
        anchor.timing.completed_at,
    ]);
    if let Some(observed) = anchor.timing.observed_at {
        boundary_times.insert(observed);
    }
    for at in boundary_times {
        append_observation_tx(
            transaction,
            &TimelineObservation::new(
                anchor.session_id,
                anchor.target_id,
                at,
                None,
                persisted_at,
                ObservationKind::InteractionBoundary,
                ObservationPayloadRef::Interaction(anchor.interaction_id),
            )
            .map_err(|_| persistence_error("interaction boundary evidence is invalid"))?,
            None,
        )?;
    }
    if let Some(navigation_id) = navigation_id {
        append_observation_tx(
            transaction,
            &TimelineObservation::new(
                anchor.session_id,
                anchor.target_id,
                anchor.timing.completed_at,
                None,
                persisted_at,
                ObservationKind::Navigation,
                ObservationPayloadRef::Navigation(navigation_id),
            )
            .map_err(|_| persistence_error("navigation evidence is invalid"))?,
            None,
        )?;
    }
    Ok(())
}

impl SqliteIndex {
    pub(crate) fn append_operation_evidence(
        &self,
        anchor: &InteractionAnchor,
        record: Option<&InteractionRecord>,
        persisted_at: ObservedTime,
        navigation_id: Option<NavigationId>,
    ) -> krometrail_core::Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin operation evidence persistence"))?;
        append_operation_evidence_tx(&transaction, anchor, record, persisted_at, navigation_id)?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit operation evidence"))
    }

    fn read_interaction(
        &self,
        interaction_id: InteractionId,
    ) -> krometrail_core::Result<Option<(InteractionAnchor, Option<InteractionRecord>)>> {
        let connection = self.connection()?;
        read_raw_by_id(&connection, codec::id(interaction_id.as_uuid()).as_ref())?
            .map(decode_interaction)
            .transpose()
    }
}

impl InteractionAnchorSource for SqliteIndex {
    fn interaction_anchor(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        Box::pin(async move {
            self.read_interaction(interaction_id)
                .map(|value| value.map(|(anchor, _)| anchor))
        })
    }

    fn latest_interaction_anchor(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let raw = connection
                .query_row(
                    "SELECT interaction_id, session_id, target_id, operation, started_time_be, \
                            dispatched_time_be, completed_time_be, observed_time_be, record_json \
                     FROM interactions WHERE session_id=?1 AND target_id=?2 \
                     ORDER BY COALESCE(observed_time_be, completed_time_be) DESC, \
                              completed_time_be DESC, dispatched_time_be DESC, \
                              started_time_be DESC, interaction_id DESC LIMIT 1",
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec(),
                    ],
                    raw_interaction,
                )
                .optional()
                .map_err(|_| persistence_error("could not query latest interaction evidence"))?;
            raw.map(decode_interaction)
                .transpose()
                .map(|value| value.map(|(anchor, _)| anchor))
        })
    }
}

impl InteractionRecordSource for SqliteIndex {
    fn interaction_record(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionRecord>>> {
        Box::pin(async move {
            self.read_interaction(interaction_id)
                .map(|value| value.and_then(|(_, record)| record))
        })
    }
}

struct RawInteraction {
    interaction_id: Vec<u8>,
    session_id: Vec<u8>,
    target_id: Vec<u8>,
    operation: String,
    started_at: Vec<u8>,
    dispatched_at: Vec<u8>,
    completed_at: Vec<u8>,
    observed_at: Option<Vec<u8>>,
    record_json: Option<String>,
}

fn raw_interaction(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawInteraction> {
    Ok(RawInteraction {
        interaction_id: row.get(0)?,
        session_id: row.get(1)?,
        target_id: row.get(2)?,
        operation: row.get(3)?,
        started_at: row.get(4)?,
        dispatched_at: row.get(5)?,
        completed_at: row.get(6)?,
        observed_at: row.get(7)?,
        record_json: row.get(8)?,
    })
}

fn read_raw_by_id(
    connection: &rusqlite::Connection,
    key: &[u8],
) -> krometrail_core::Result<Option<RawInteraction>> {
    connection
        .query_row(
            "SELECT interaction_id, session_id, target_id, operation, started_time_be, \
                    dispatched_time_be, completed_time_be, observed_time_be, record_json \
             FROM interactions WHERE interaction_id=?1",
            params![key],
            raw_interaction,
        )
        .optional()
        .map_err(|_| persistence_error("could not query interaction evidence"))
}

fn decode_interaction(
    raw: RawInteraction,
) -> krometrail_core::Result<(InteractionAnchor, Option<InteractionRecord>)> {
    let operation = BrowserOperationKind::from_stable_name(&raw.operation)
        .ok_or_else(|| persistence_error("stored interaction operation is unknown"))?;
    let anchor = InteractionAnchor::new(
        InteractionId::from_uuid(codec::decode_id(&raw.interaction_id)?),
        SessionId::from_uuid(codec::decode_id(&raw.session_id)?),
        TargetId::from_uuid(codec::decode_id(&raw.target_id)?),
        operation,
        InteractionTiming::new(
            SessionTime::from_nanos(codec::decode_u64(&raw.started_at)?),
            SessionTime::from_nanos(codec::decode_u64(&raw.dispatched_at)?),
            SessionTime::from_nanos(codec::decode_u64(&raw.completed_at)?),
            raw.observed_at
                .as_deref()
                .map(codec::decode_u64)
                .transpose()?
                .map(SessionTime::from_nanos),
        )
        .map_err(|_| persistence_error("stored interaction timing is invalid"))?,
    )
    .map_err(|_| persistence_error("stored interaction anchor is invalid"))?;
    let record = raw
        .record_json
        .as_deref()
        .map(serde_json::from_str::<InteractionRecord>)
        .transpose()
        .map_err(|_| persistence_error("stored interaction record is malformed"))?;
    if let Some(record) = &record {
        let record_anchor = record
            .anchor()
            .map_err(|_| persistence_error("stored interaction record is invalid"))?;
        if record_anchor != anchor {
            return Err(persistence_error(
                "stored interaction record does not match its anchor",
            ));
        }
    }
    Ok((anchor, record))
}
