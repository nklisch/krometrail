use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{GapId, SessionId, TargetId},
    time::{ObservedTime, SessionRange},
    validation::deserialize_validated,
};

define_stable_enum! {
    pub enum CaptureGapReason {
        IngestionQueueSaturated => "ingestion_queue_saturated",
        PersistenceRejected => "persistence_rejected",
        AcknowledgementFailed => "acknowledgement_failed",
        TargetHidden => "target_hidden",
        ScreencastPaused => "screencast_paused",
        BrowserDisconnected => "browser_disconnected",
        CaptureStopped => "capture_stopped",
        FrameRejected => "frame_rejected",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureGap {
    id: GapId,
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    observed_time: ObservedTime,
    reason: CaptureGapReason,
    estimated_missing_frames: Option<NonZeroU64>,
    detail: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CaptureGapWire {
    id: GapId,
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    observed_time: ObservedTime,
    reason: CaptureGapReason,
    estimated_missing_frames: Option<NonZeroU64>,
    detail: Option<String>,
}

impl CaptureGap {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: GapId,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        observed_time: ObservedTime,
        reason: CaptureGapReason,
        estimated_missing_frames: Option<NonZeroU64>,
        detail: Option<String>,
    ) -> Result<Self> {
        let gap = Self {
            id,
            session_id,
            target_id,
            range,
            observed_time,
            reason,
            estimated_missing_frames,
            detail,
        };
        gap.validate()?;
        Ok(gap)
    }

    pub const fn id(&self) -> GapId {
        self.id
    }
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub const fn target_id(&self) -> TargetId {
        self.target_id
    }
    pub const fn range(&self) -> SessionRange {
        self.range
    }
    pub const fn observed_time(&self) -> ObservedTime {
        self.observed_time
    }
    pub const fn reason(&self) -> &CaptureGapReason {
        &self.reason
    }
    pub const fn estimated_missing_frames(&self) -> Option<NonZeroU64> {
        self.estimated_missing_frames
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    fn validate(&self) -> Result<()> {
        if self.range.end().as_nanos() > self.observed_time.as_nanos() {
            return Err(invalid(
                "capture gap range must not end after its declaration time",
            ));
        }
        if self
            .detail
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(invalid(
                "capture gap detail must not be empty or whitespace-only",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for CaptureGap {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: CaptureGapWire| {
            Self::new(
                wire.id,
                wire.session_id,
                wire.target_id,
                wire.range,
                wire.observed_time,
                wire.reason,
                wire.estimated_missing_frames,
                wire.detail,
            )
        })
    }
}

crate::validation::delegate_json_schema!(CaptureGap => CaptureGapWire);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::*,
        time::{SessionRange, SessionTime},
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn every_gap_reason_round_trips_with_its_stable_name() {
        for reason in CaptureGapReason::ALL {
            let encoded = serde_json::to_string(reason).unwrap();
            assert_eq!(encoded, format!("\"{}\"", reason.as_str()));
            assert_eq!(
                serde_json::from_str::<CaptureGapReason>(&encoded).unwrap(),
                *reason
            );
        }
    }

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
                ObservedTime::from_nanos(1),
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
                ObservedTime::from_nanos(1),
                CaptureGapReason::CaptureStopped,
                NonZeroU64::new(2),
                None
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_malformed_serialized_gap_details_and_ranges() {
        let value = serde_json::json!({
            "id": UUID, "session_id": UUID, "target_id": UUID,
            "range": {"start": 2, "end": 1}, "observed_time": 2, "reason": "capture_stopped",
            "estimated_missing_frames": null, "detail": null
        });
        assert!(serde_json::from_value::<CaptureGap>(value).is_err());
        let value = serde_json::json!({
            "id": UUID, "session_id": UUID, "target_id": UUID,
            "range": {"start": 1, "end": 2}, "observed_time": 2, "reason": "capture_stopped",
            "estimated_missing_frames": null, "detail": " "
        });
        assert!(serde_json::from_value::<CaptureGap>(value).is_err());
        let valid = CaptureGap::new(
            GapId::from_uuid(UUID.parse().unwrap()),
            SessionId::from_uuid(UUID.parse().unwrap()),
            TargetId::from_uuid(UUID.parse().unwrap()),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
            ObservedTime::from_nanos(1),
            CaptureGapReason::CaptureStopped,
            NonZeroU64::new(2),
            Some("browser stopped capture".into()),
        )
        .unwrap();
        assert!(
            CaptureGap::new(
                valid.id(),
                valid.session_id(),
                valid.target_id(),
                SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
                ObservedTime::from_nanos(1),
                CaptureGapReason::CaptureStopped,
                None,
                None,
            )
            .is_err()
        );
        let encoded = serde_json::to_string(&valid).unwrap();
        assert_eq!(serde_json::from_str::<CaptureGap>(&encoded).unwrap(), valid);
    }
}
