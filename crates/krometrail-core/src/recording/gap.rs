use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{GapId, SessionId, TargetId},
    time::SessionRange,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureGapReason {
    IngestionQueueSaturated,
    PersistenceRejected,
    SourceSequenceDiscontinuity,
    TargetHidden,
    ScreencastPaused,
    BrowserDisconnected,
    CaptureStopped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureGap {
    pub id: GapId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub range: SessionRange,
    pub reason: CaptureGapReason,
    pub estimated_missing_frames: Option<NonZeroU64>,
    pub detail: Option<String>,
}

impl CaptureGap {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: GapId,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        reason: CaptureGapReason,
        estimated_missing_frames: Option<NonZeroU64>,
        detail: Option<String>,
    ) -> Result<Self> {
        if detail
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(invalid(
                "capture gap detail must not be empty or whitespace-only",
            ));
        }
        Ok(Self {
            id,
            session_id,
            target_id,
            range,
            reason,
            estimated_missing_frames,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::*,
        time::{SessionRange, SessionTime},
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn gap_is_explicit_and_rejects_empty_detail() {
        let id = GapId::from_uuid(UUID.parse().unwrap());
        let session = SessionId::from_uuid(UUID.parse().unwrap());
        let target = TargetId::from_uuid(UUID.parse().unwrap());
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap();
        assert!(
            CaptureGap::new(
                id,
                session,
                target,
                range,
                CaptureGapReason::CaptureStopped,
                None,
                Some(" ".into())
            )
            .is_err()
        );
        assert!(
            CaptureGap::new(
                id,
                session,
                target,
                range,
                CaptureGapReason::CaptureStopped,
                NonZeroU64::new(2),
                None
            )
            .is_ok()
        );
    }
}
