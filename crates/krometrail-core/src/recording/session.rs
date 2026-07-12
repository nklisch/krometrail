use std::{num::NonZeroU64, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::{
    browser::{BrowserVersion, ProfileIdentity},
    capabilities::{CapabilityId, validate_capability_selection},
    error::{Result, invalid},
    ids::SessionId,
    lifecycle::SessionLifecycle,
    time::ObservedTime,
    validation::deserialize_validated,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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

impl<'de> Deserialize<'de> for DiskBudgetBytes {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: u64| Self::new(value))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct CaptureStatistics {
    received_frames: u64,
    accepted_frames: u64,
    dropped_frames: u64,
    persisted_frames: u64,
    gap_count: u64,
}

#[derive(Deserialize)]
struct CaptureStatisticsWire {
    received_frames: u64,
    accepted_frames: u64,
    dropped_frames: u64,
    persisted_frames: u64,
    gap_count: u64,
}

impl CaptureStatistics {
    pub fn new(
        received_frames: u64,
        accepted_frames: u64,
        dropped_frames: u64,
        persisted_frames: u64,
        gap_count: u64,
    ) -> Result<Self> {
        let statistics = Self {
            received_frames,
            accepted_frames,
            dropped_frames,
            persisted_frames,
            gap_count,
        };
        statistics.validate()
    }

    pub const fn received_frames(&self) -> u64 {
        self.received_frames
    }

    pub const fn accepted_frames(&self) -> u64 {
        self.accepted_frames
    }

    pub const fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    pub const fn persisted_frames(&self) -> u64 {
        self.persisted_frames
    }

    pub const fn gap_count(&self) -> u64 {
        self.gap_count
    }

    /// Replace all counters atomically, leaving the previous valid value intact on error.
    pub fn update(
        &mut self,
        received_frames: u64,
        accepted_frames: u64,
        dropped_frames: u64,
        persisted_frames: u64,
        gap_count: u64,
    ) -> Result<()> {
        *self = Self::new(
            received_frames,
            accepted_frames,
            dropped_frames,
            persisted_frames,
            gap_count,
        )?;
        Ok(())
    }

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

impl<'de> Deserialize<'de> for CaptureStatistics {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: CaptureStatisticsWire| {
            Self::new(
                wire.received_frames,
                wire.accepted_frames,
                wire.dropped_frames,
                wire.persisted_frames,
                wire.gap_count,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordingSession {
    id: SessionId,
    origin: ObservedTime,
    started_at: SystemTime,
    ended_at: Option<SystemTime>,
    browser: BrowserVersion,
    profile: ProfileIdentity,
    lifecycle: SessionLifecycle,
    disk_budget: DiskBudgetBytes,
    capabilities: Vec<CapabilityId>,
    statistics: CaptureStatistics,
}

#[derive(Deserialize)]
struct RecordingSessionWire {
    id: SessionId,
    origin: ObservedTime,
    started_at: SystemTime,
    ended_at: Option<SystemTime>,
    browser: BrowserVersion,
    profile: ProfileIdentity,
    lifecycle: SessionLifecycle,
    disk_budget: DiskBudgetBytes,
    capabilities: Vec<CapabilityId>,
    statistics: CaptureStatistics,
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
        Self::from_parts(
            id,
            origin,
            started_at,
            None,
            browser,
            profile,
            SessionLifecycle::Starting,
            disk_budget,
            capabilities,
            CaptureStatistics::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        id: SessionId,
        origin: ObservedTime,
        started_at: SystemTime,
        ended_at: Option<SystemTime>,
        browser: BrowserVersion,
        profile: ProfileIdentity,
        lifecycle: SessionLifecycle,
        disk_budget: DiskBudgetBytes,
        capabilities: Vec<CapabilityId>,
        statistics: CaptureStatistics,
    ) -> Result<Self> {
        let session = Self {
            id,
            origin,
            started_at,
            ended_at,
            browser,
            profile,
            lifecycle,
            disk_budget,
            capabilities,
            statistics,
        };
        session.validate()?;
        Ok(session)
    }

    fn validate(&self) -> Result<()> {
        self.browser.validate()?;
        validate_capability_selection(&self.capabilities)?;
        self.statistics.validate()?;
        Self::validate_lifecycle_end_state(self.lifecycle, self.started_at, self.ended_at)
    }

    fn validate_lifecycle_end_state(
        lifecycle: SessionLifecycle,
        started_at: SystemTime,
        ended_at: Option<SystemTime>,
    ) -> Result<()> {
        match (lifecycle, ended_at) {
            (SessionLifecycle::Ended, Some(end)) if end >= started_at => Ok(()),
            (SessionLifecycle::Ended, Some(_)) => {
                Err(invalid("session end time must not precede its start time"))
            }
            (SessionLifecycle::Ended, None) => Err(invalid("ended sessions require an end time")),
            (_, Some(_)) => Err(invalid("only an ended session may set an end time")),
            (_, None) => Ok(()),
        }
    }

    pub const fn id(&self) -> SessionId {
        self.id
    }

    pub const fn origin(&self) -> ObservedTime {
        self.origin
    }

    pub const fn started_at(&self) -> SystemTime {
        self.started_at
    }

    pub const fn ended_at(&self) -> Option<SystemTime> {
        self.ended_at
    }

    pub fn browser(&self) -> &BrowserVersion {
        &self.browser
    }

    pub fn profile(&self) -> &ProfileIdentity {
        &self.profile
    }

    pub const fn lifecycle(&self) -> SessionLifecycle {
        self.lifecycle
    }

    pub const fn disk_budget(&self) -> DiskBudgetBytes {
        self.disk_budget
    }

    pub fn capabilities(&self) -> &[CapabilityId] {
        &self.capabilities
    }

    pub const fn statistics(&self) -> &CaptureStatistics {
        &self.statistics
    }

    pub fn set_statistics(&mut self, statistics: CaptureStatistics) -> Result<()> {
        self.statistics = statistics.validate()?;
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: SessionLifecycle,
        ended_at: Option<SystemTime>,
    ) -> Result<()> {
        self.lifecycle.transition(next)?;
        Self::validate_lifecycle_end_state(next, self.started_at, ended_at)?;
        self.lifecycle = next;
        self.ended_at = ended_at;
        Ok(())
    }

    pub fn validate_statistics(&self) -> Result<()> {
        self.statistics.validate().map(|_| ())
    }
}

impl<'de> Deserialize<'de> for RecordingSession {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: RecordingSessionWire| {
            Self::from_parts(
                wire.id,
                wire.origin,
                wire.started_at,
                wire.ended_at,
                wire.browser,
                wire.profile,
                wire.lifecycle,
                wire.disk_budget,
                wire.capabilities,
                wire.statistics,
            )
        })
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
        assert!(CaptureStatistics::new(1, 1, 1, 0, 0).is_err());
        assert!(CaptureStatistics::new(1, 1, 0, 2, 0).is_err());
    }

    #[test]
    fn statistics_are_readable_and_mutated_atomically() {
        let mut statistics = CaptureStatistics::new(2, 1, 1, 1, 0).unwrap();
        assert_eq!(statistics.received_frames(), 2);
        assert!(statistics.update(1, 1, 1, 0, 0).is_err());
        assert_eq!(statistics.received_frames(), 2);
        statistics.update(3, 2, 1, 2, 1).unwrap();
        assert_eq!(statistics.persisted_frames(), 2);
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

    #[test]
    fn rejects_malformed_serialized_statistics_and_sessions() {
        let malformed_statistics = r#"{"received_frames":1,"accepted_frames":1,"dropped_frames":1,"persisted_frames":0,"gap_count":0}"#;
        assert!(serde_json::from_str::<CaptureStatistics>(malformed_statistics).is_err());
        let malformed_session = format!(
            r#"{{"id":"{UUID}","origin":1,"started_at":{{"secs_since_epoch":0,"nanos_since_epoch":0}},"ended_at":null,"browser":{{"product":"Chrome","revision":"r","protocol":"p"}},"profile":"profile","lifecycle":"ended","disk_budget":1024,"capabilities":["control"],"statistics":{{"received_frames":0,"accepted_frames":0,"dropped_frames":0,"persisted_frames":0,"gap_count":0}}}}"#
        );
        assert!(serde_json::from_str::<RecordingSession>(&malformed_session).is_err());
        let valid = session();
        let encoded = serde_json::to_string(&valid).unwrap();
        assert_eq!(
            serde_json::from_str::<RecordingSession>(&encoded).unwrap(),
            valid
        );
    }
}
