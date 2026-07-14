use crate::{PageTarget, RecordingSession, Result, SessionId};

use super::PortFuture;

/// Persists validated session and target catalog records.
pub trait RecordingCatalog: Send + Sync {
    fn put_session(&self, session: RecordingSession) -> PortFuture<'_, Result<()>>;
    fn put_target(&self, session_id: SessionId, target: PageTarget) -> PortFuture<'_, Result<()>>;
    fn session(&self, session_id: SessionId) -> PortFuture<'_, Result<Option<RecordingSession>>>;
    fn target(
        &self,
        session_id: SessionId,
        target_id: crate::TargetId,
    ) -> PortFuture<'_, Result<Option<PageTarget>>>;
}
