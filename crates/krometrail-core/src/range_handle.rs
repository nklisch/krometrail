use crate::{PortFuture, ResolvedRange, ResolvedRangeHandleId, Result, SessionId};

pub const MAX_RESOLVED_RANGE_HANDLES: usize = 4_096;
/// Aggregate admission budget measured from each range's complete serialized contract.
/// This bounds all variable-length identifiers, warnings, gaps, and gap-detail text.
pub const MAX_RESOLVED_RANGE_HANDLE_BUDGET_BYTES: usize = 16 * 1024 * 1024;

/// Process-local convenience authority for exact resolved temporal ranges.
///
/// Implementations may cache immutable range values, but retained source evidence remains the
/// authority and must be revalidated before a range is returned.
pub trait ResolvedRangeHandles: Send + Sync {
    /// Admit an immutable range only after its retained source metadata is available and ordered.
    fn register(&self, range: ResolvedRange) -> PortFuture<'_, Result<ResolvedRangeHandleId>>;

    fn resolve_available(
        &self,
        handle: ResolvedRangeHandleId,
    ) -> PortFuture<'_, Result<ResolvedRange>>;

    fn invalidate_session(&self, session_id: SessionId) -> Result<usize>;
}
