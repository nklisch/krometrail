use krometrail_core::{
    DiskBudgetBytes, PinChange, PortFuture, RetentionRange, RetentionStatus, RetentionStore,
    SessionDeletion, SessionId,
};

/// Retention seam for capture tests that are intentionally unrelated to disk budgets.
#[allow(dead_code)]
pub struct AlwaysAvailableRetention;

impl RetentionStore for AlwaysAvailableRetention {
    fn pin_range(
        &self,
        request: RetentionRange,
    ) -> PortFuture<'_, krometrail_core::Result<PinChange>> {
        Box::pin(std::future::ready(Ok(PinChange {
            request,
            protected_segments: Vec::new(),
            pinned_usage_bytes: 0,
        })))
    }

    fn unpin_range(
        &self,
        request: RetentionRange,
    ) -> PortFuture<'_, krometrail_core::Result<PinChange>> {
        self.pin_range(request)
    }

    fn enforce_budget(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        self.status()
    }

    fn status(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        Box::pin(std::future::ready(Ok(RetentionStatus::empty(
            DiskBudgetBytes::default(),
        ))))
    }

    fn delete_session(
        &self,
        session_id: SessionId,
    ) -> PortFuture<'_, krometrail_core::Result<SessionDeletion>> {
        Box::pin(std::future::ready(Ok(SessionDeletion {
            session_id,
            removed_segments: 0,
            removed_frames: 0,
            removed_artifacts: 0,
            removed_bytes: 0,
        })))
    }

    fn wait_until_recording_allowed(&self) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }
}
