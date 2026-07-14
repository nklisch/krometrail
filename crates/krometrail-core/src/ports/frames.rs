use crate::{EncodedFrame, FrameId, Result, SessionId, SessionRange, TargetId};

use super::PortFuture;

/// Reads retained encoded frames without exposing their physical storage.
pub trait FrameSource: Send + Sync {
    /// Returns exactly one frame per id in request order; any missing id fails the request.
    fn frames_by_id(&self, frame_ids: Vec<FrameId>) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;

    /// Returns retained frames for one target in capture-ordinal order.
    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;
}
