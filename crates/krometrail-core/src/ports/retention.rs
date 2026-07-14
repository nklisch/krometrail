use crate::{
    ErrorCode, KrometrailError, NonEmptyText, PinState, ProgressivePinChange, Result,
    RetentionPinRequest, SessionId,
    recording::{PinChange, RetentionRange, RetentionStatus, SessionDeletion},
};

use super::PortFuture;

/// Domain-facing retention operations. Implementations coordinate metadata and
/// physical storage without exposing either representation to callers.
pub trait RetentionStore: Send + Sync {
    /// Atomically validates and protects every expected frame in a resolved range.
    fn pin_resolved_range(
        &self,
        _request: RetentionPinRequest,
    ) -> PortFuture<'_, Result<ProgressivePinChange>> {
        Box::pin(std::future::ready(Err(progressive_pin_unsupported())))
    }

    /// Removes only the exact pin and reports final post-budget overlap truth.
    fn unpin_resolved_range(
        &self,
        _request: RetentionPinRequest,
    ) -> PortFuture<'_, Result<ProgressivePinChange>> {
        Box::pin(std::future::ready(Err(progressive_pin_unsupported())))
    }

    fn query_pin_state(&self, _request: RetentionPinRequest) -> PortFuture<'_, Result<PinState>> {
        Box::pin(std::future::ready(Err(progressive_pin_unsupported())))
    }

    fn pin_range(&self, request: RetentionRange) -> PortFuture<'_, Result<PinChange>>;
    fn unpin_range(&self, request: RetentionRange) -> PortFuture<'_, Result<PinChange>>;
    fn enforce_budget(&self) -> PortFuture<'_, Result<RetentionStatus>>;
    fn status(&self) -> PortFuture<'_, Result<RetentionStatus>>;
    fn delete_session(&self, session_id: SessionId) -> PortFuture<'_, Result<SessionDeletion>>;
    fn wait_until_recording_allowed(&self) -> PortFuture<'_, Result<()>>;
}

fn progressive_pin_unsupported() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Unsupported,
        NonEmptyText::new("this retention store does not provide resolved-range pin reporting")
            .expect("static progressive pin error is non-empty"),
    )
}
