use std::sync::Arc;

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
    /// Returns observations in the inclusive range using a deterministic adapter-defined order.
    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<TimelineObservation>>>;
}

impl<T: TimelineStore + ?Sized> TimelineStore for Arc<T> {
    fn append(&self, observation: TimelineObservation) -> PortFuture<'_, Result<()>> {
        (**self).append(observation)
    }
    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<TimelineObservation>>> {
        (**self).range(session_id, target_id, range)
    }
}
