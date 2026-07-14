use crate::{
    Result, SessionId,
    recording::{PinChange, RetentionRange, RetentionStatus, SessionDeletion},
};

use super::PortFuture;

/// Domain-facing retention operations. Implementations coordinate metadata and
/// physical storage without exposing either representation to callers.
pub trait RetentionStore: Send + Sync {
    fn pin_range(&self, request: RetentionRange) -> PortFuture<'_, Result<PinChange>>;
    fn unpin_range(&self, request: RetentionRange) -> PortFuture<'_, Result<PinChange>>;
    fn enforce_budget(&self) -> PortFuture<'_, Result<RetentionStatus>>;
    fn status(&self) -> PortFuture<'_, Result<RetentionStatus>>;
    fn delete_session(&self, session_id: SessionId) -> PortFuture<'_, Result<SessionDeletion>>;
    fn wait_until_recording_allowed(&self) -> PortFuture<'_, Result<()>>;
}
