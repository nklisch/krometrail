use std::sync::Arc;

use crate::{CaptureGap, Result, SessionId, SessionRange, TargetId};

use super::PortFuture;

/// Persists explicit capture-loss intervals independently from frame payloads.
///
/// Implementations of [`Self::gaps`] return only gaps whose inclusive interval
/// intersects the requested range. Callers must therefore validate and clip
/// returned observations at their own temporal boundary; a non-intersecting
/// persisted gap is not required to be returned.
pub trait CaptureGapStore: Send + Sync {
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn gaps(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<CaptureGap>>>;
}

impl<T: CaptureGapStore + ?Sized> CaptureGapStore for Arc<T> {
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>> {
        (**self).append_gap(gap)
    }
    fn gaps(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<CaptureGap>>> {
        (**self).gaps(session_id, target_id, range)
    }
}
