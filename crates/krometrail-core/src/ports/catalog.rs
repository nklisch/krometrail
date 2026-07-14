use std::sync::Arc;

use crate::{PageTarget, RecordingSession, Result, SessionId, TargetId};

use super::PortFuture;

/// Persists validated session and target catalog records.
pub trait RecordingCatalog: Send + Sync {
    fn put_session(&self, session: RecordingSession) -> PortFuture<'_, Result<()>>;
    fn put_target(&self, session_id: SessionId, target: PageTarget) -> PortFuture<'_, Result<()>>;
    fn session(&self, session_id: SessionId) -> PortFuture<'_, Result<Option<RecordingSession>>>;
    fn target(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<Option<PageTarget>>>;
}

impl<T: RecordingCatalog + ?Sized> RecordingCatalog for Arc<T> {
    fn put_session(&self, session: RecordingSession) -> PortFuture<'_, Result<()>> {
        (**self).put_session(session)
    }
    fn put_target(&self, session_id: SessionId, target: PageTarget) -> PortFuture<'_, Result<()>> {
        (**self).put_target(session_id, target)
    }
    fn session(&self, session_id: SessionId) -> PortFuture<'_, Result<Option<RecordingSession>>> {
        (**self).session(session_id)
    }
    fn target(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<Option<PageTarget>>> {
        (**self).target(session_id, target_id)
    }
}
