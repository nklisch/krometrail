use std::{num::NonZeroU64, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::{
    browser::{BrowserVersion, ProfileIdentity},
    capabilities::{CapabilityId, validate_capability_selection},
    error::{Result, invalid},
    ids::SessionId,
    lifecycle::SessionLifecycle,
    time::ObservedTime,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiskBudgetBytes(NonZeroU64);

impl DiskBudgetBytes {
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| invalid("disk budget must be greater than zero"))
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureStatistics {
    pub received_frames: u64,
    pub accepted_frames: u64,
    pub dropped_frames: u64,
    pub persisted_frames: u64,
    pub gap_count: u64,
}

impl CaptureStatistics {
    pub fn validate(self) -> Result<Self> {
        let accounted = self
            .accepted_frames
            .checked_add(self.dropped_frames)
            .ok_or_else(|| invalid("capture frame statistics overflow"))?;
        if accounted > self.received_frames {
            return Err(invalid(
                "accepted and dropped frames exceed received frames",
            ));
        }
        if self.persisted_frames > self.accepted_frames {
            return Err(invalid("persisted frames exceed accepted frames"));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordingSession {
    pub id: SessionId,
    pub origin: ObservedTime,
    pub started_at: SystemTime,
    pub ended_at: Option<SystemTime>,
    pub browser: BrowserVersion,
    pub profile: ProfileIdentity,
    pub lifecycle: SessionLifecycle,
    pub disk_budget: DiskBudgetBytes,
    pub capabilities: Vec<CapabilityId>,
    pub statistics: CaptureStatistics,
}

impl RecordingSession {
    pub fn new(
        id: SessionId,
        origin: ObservedTime,
        started_at: SystemTime,
        browser: BrowserVersion,
        profile: ProfileIdentity,
        disk_budget: DiskBudgetBytes,
        capabilities: Vec<CapabilityId>,
    ) -> Result<Self> {
        browser.validate()?;
        validate_capability_selection(&capabilities)?;
        Ok(Self {
            id,
            origin,
            started_at,
            ended_at: None,
            browser,
            profile,
            lifecycle: SessionLifecycle::Starting,
            disk_budget,
            capabilities,
            statistics: CaptureStatistics::default(),
        })
    }

    pub fn transition(
        &mut self,
        next: SessionLifecycle,
        ended_at: Option<SystemTime>,
    ) -> Result<()> {
        self.lifecycle.transition(next)?;
        match (next, ended_at) {
            (SessionLifecycle::Ended, Some(end)) => {
                if end < self.started_at {
                    return Err(invalid("session end time must not precede its start time"));
                }
                self.ended_at = Some(end);
            }
            (SessionLifecycle::Ended, None) => {
                return Err(invalid("ended sessions require an end time"));
            }
            (_, Some(_)) => return Err(invalid("only an ended session may set an end time")),
            (_, None) => {}
        }
        self.lifecycle = next;
        Ok(())
    }

    pub fn validate_statistics(&self) -> Result<()> {
        self.statistics.validate().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{browser::BrowserVersion, ids::SessionId};

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn session() -> RecordingSession {
        RecordingSession::new(
            SessionId::from_uuid(UUID.parse().unwrap()),
            ObservedTime::from_nanos(1),
            SystemTime::UNIX_EPOCH,
            BrowserVersion::new("Chrome", "revision", "1").unwrap(),
            ProfileIdentity::new("profile").unwrap(),
            DiskBudgetBytes::new(1024).unwrap(),
            vec![CapabilityId::Control],
        )
        .unwrap()
    }

    #[test]
    fn rejects_zero_budget_and_inconsistent_statistics() {
        assert!(DiskBudgetBytes::new(0).is_err());
        assert!(
            CaptureStatistics {
                received_frames: 1,
                accepted_frames: 1,
                dropped_frames: 1,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            CaptureStatistics {
                received_frames: 1,
                accepted_frames: 1,
                persisted_frames: 2,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn lifecycle_and_end_time_are_validated() {
        let mut session = session();
        assert!(session.transition(SessionLifecycle::Ended, None).is_err());
        session
            .transition(SessionLifecycle::Recording, None)
            .unwrap();
        assert!(
            session
                .transition(SessionLifecycle::Recording, None)
                .is_err()
        );
        assert!(
            session
                .transition(SessionLifecycle::Stopping, Some(SystemTime::UNIX_EPOCH))
                .is_err()
        );
        session
            .transition(SessionLifecycle::Stopping, None)
            .unwrap();
        session
            .transition(
                SessionLifecycle::Ended,
                Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1)),
            )
            .unwrap();
        assert!(
            session
                .transition(SessionLifecycle::Recording, None)
                .is_err()
        );
    }
}
