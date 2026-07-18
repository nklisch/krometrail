use crate::{PortFuture, ResolvedRange, ResolvedRangeHandleId, Result, SessionId};

pub const MAX_RESOLVED_RANGE_HANDLES: usize = 4_096;

/// Process-local convenience authority for exact resolved temporal ranges.
///
/// Implementations may cache immutable range values, but retained source evidence remains the
/// authority and must be revalidated before a range is returned.
pub trait ResolvedRangeHandles: Send + Sync {
    fn register(&self, range: ResolvedRange) -> Result<ResolvedRangeHandleId>;

    fn resolve_available(
        &self,
        handle: ResolvedRangeHandleId,
    ) -> PortFuture<'_, Result<ResolvedRange>>;

    fn invalidate_session(&self, session_id: SessionId) -> Result<usize>;
}
