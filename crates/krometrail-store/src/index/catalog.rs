use krometrail_core::{PageTarget, PortFuture, RecordingCatalog, RecordingSession, SessionId, TargetId};
use rusqlite::{OptionalExtension, params};

use super::{SqliteIndex, codec, ensure_session};
use crate::persistence_error;

impl RecordingCatalog for SqliteIndex {
    fn put_session(
        &self,
        session: RecordingSession,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let json = serde_json::to_string(&session)
                .map_err(|_| persistence_error("could not encode session metadata"))?;
            let connection = self.connection()?;
            connection
                .execute(
                    "INSERT INTO sessions(session_id, record_json) VALUES (?1, ?2) \
                     ON CONFLICT(session_id) DO UPDATE SET record_json=excluded.record_json",
                    params![codec::id(session.id().as_uuid()), json],
                )
                .map_err(|_| persistence_error("could not persist session metadata"))?;
            Ok(())
        })
    }

    fn session(
        &self,
        session_id: SessionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<RecordingSession>>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let json: Option<String> = connection
                .query_row(
                    "SELECT record_json FROM sessions WHERE session_id=?1",
                    params![codec::id(session_id.as_uuid()).to_vec()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| persistence_error("could not query session metadata"))?
                .flatten();
            let Some(json) = json else { return Ok(None) };
            let session: RecordingSession = serde_json::from_str(&json)
                .map_err(|_| persistence_error("stored session metadata is malformed"))?;
            if session.id() != session_id {
                return Err(persistence_error("stored session metadata has the wrong identity"));
            }
            Ok(Some(session))
        })
    }

    fn target(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<PageTarget>>> {
        Box::pin(async move {
            let connection = self.connection()?;
            let json: Option<String> = connection
                .query_row(
                    "SELECT record_json FROM targets WHERE session_id=?1 AND target_id=?2",
                    params![
                        codec::id(session_id.as_uuid()).to_vec(),
                        codec::id(target_id.as_uuid()).to_vec()
                    ],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| persistence_error("could not query target metadata"))?
                .flatten();
            let Some(json) = json else { return Ok(None) };
            let target: PageTarget = serde_json::from_str(&json)
                .map_err(|_| persistence_error("stored target metadata is malformed"))?;
            if target.id() != target_id {
                return Err(persistence_error("stored target metadata has the wrong identity"));
            }
            Ok(Some(target))
        })
    }

    fn put_target(
        &self,
        session_id: SessionId,
        target: PageTarget,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let json = serde_json::to_string(&target)
                .map_err(|_| persistence_error("could not encode target metadata"))?;
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin target metadata persistence"))?;
            ensure_session(&transaction, session_id)?;
            transaction
                .execute(
                    "INSERT INTO targets(session_id, target_id, record_json) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(session_id, target_id) DO UPDATE SET record_json=excluded.record_json",
                    params![
                        codec::id(session_id.as_uuid()),
                        codec::id(target.id().as_uuid()),
                        json
                    ],
                )
                .map_err(|_| persistence_error("could not persist target metadata"))?;
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit target metadata"))
        })
    }
}
