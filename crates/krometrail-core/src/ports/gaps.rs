use crate::{CaptureGap, Result, SessionId, SessionRange, TargetId};

use super::PortFuture;

/// Persists explicit capture-loss intervals independently from frame payloads.
pub trait CaptureGapStore: Send + Sync {
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn gaps(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<CaptureGap>>>;
}
