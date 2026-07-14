use krometrail_core::{
    AnchorScope, InteractionAnchor, InteractionAnchorSource, InteractionId, ObservationKind,
    ObservationPayloadRef, PortFuture, SessionId, TargetId, TimelineAnchorSource,
    TimelineObservation,
};
use rusqlite::{OptionalExtension, params};

use super::{SqliteIndex, codec, timeline::{RawObservation, decode_observation}};
use crate::persistence_error;

impl TimelineAnchorSource for SqliteIndex {
    fn observation_for_payload(
        &self,
        scope: AnchorScope,
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
                    | (ObservationKind::Navigation, ObservationPayloadRef::Navigation(_))
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
                    "SELECT session_id, target_id, session_time_be, source_time_be,\
                            observed_time_be, kind, payload_json\
                     FROM timeline_observations\
                     WHERE kind=?1 AND payload_json=?2\
                       AND (?3 IS NULL OR session_id=?3)\
                       AND (?4 IS NULL OR target_id=?4)\
                     ORDER BY session_time_be ASC, observed_time_be ASC, observation_id ASC\
                     LIMIT 1",
                    params![
                        kind.as_str(),
                        payload_json,
                        scope.session_id.map(|id| codec::id(id.as_uuid()).to_vec()),
                        scope.target_id.map(|id| codec::id(id.as_uuid()).to_vec()),
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
                    "SELECT session_id, target_id, session_time_be, source_time_be,\
                            observed_time_be, kind, payload_json\
                     FROM timeline_observations\
                     WHERE session_id=?1 AND target_id=?2 AND kind=?3\
                     ORDER BY session_time_be DESC, observed_time_be DESC, observation_id DESC\
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

impl InteractionAnchorSource for SqliteIndex {
    fn interaction_anchor(
        &self,
        _interaction_id: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        // Interaction rows belong to the browser-operation feature. Returning None is
        // deliberate: timeline observations are not a substitute for durable timing.
        Box::pin(async { Ok(None) })
    }

    fn latest_interaction_anchor(
        &self,
        _session_id: SessionId,
        _target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        Box::pin(async { Ok(None) })
    }
}
