use crate::{
    error::Result,
    ids::{SessionId, TargetId},
    time::SessionRange,
    timeline::TimelineObservation,
};

use super::PortFuture;

/// Indexes timeline metadata independently of encoded recording payloads.
pub trait TimelineStore: Send + Sync {
    fn append(&self, observation: TimelineObservation) -> PortFuture<'_, Result<()>>;
    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<TimelineObservation>>>;
}
