use crate::{
    error::Result,
    ids::SessionId,
    recording::{CaptureGap, EncodedFrame},
};

use super::PortFuture;

/// Persists frame payloads and explicit capture gaps. Timeline indexing is a
/// separate port so either concern can report and recover from its own failure.
pub trait RecordingSink: Send + Sync {
    fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, Result<()>>;
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn flush(&self, session_id: SessionId) -> PortFuture<'_, Result<()>>;
}
