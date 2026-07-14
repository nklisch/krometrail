use krometrail_core::{PageTarget, PortFuture, RecordingCatalog, RecordingSession, SessionId};
use rusqlite::params;

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
