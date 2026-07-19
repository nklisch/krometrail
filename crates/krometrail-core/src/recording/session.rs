use std::{num::NonZeroU64, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::{
    browser::{BrowserVersion, ProfileRef},
    capabilities::{CapabilityId, validate_capability_selection},
    error::{KrometrailError, Result, invalid},
    ids::SessionId,
    lifecycle::SessionLifecycle,
    ports::EveryNthFrame,
    time::ObservedTime,
    validation::deserialize_validated,
};

pub const DEFAULT_DISK_BUDGET_BYTES: u64 = 10_000_000_000;

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

impl Default for DiskBudgetBytes {
    fn default() -> Self {
        Self::new(DEFAULT_DISK_BUDGET_BYTES).expect("default disk budget is non-zero")
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
    acknowledged_frames: u64,
    accepted_frames: u64,
    dropped_frames: u64,
    persisted_frames: u64,
    gap_count: u64,
}

#[derive(Deserialize)]
struct CaptureStatisticsWire {
    received_frames: u64,
    acknowledged_frames: u64,
    accepted_frames: u64,
    dropped_frames: u64,
    persisted_frames: u64,
    gap_count: u64,
}

impl CaptureStatistics {
    pub fn new(
        received_frames: u64,
        acknowledged_frames: u64,
        accepted_frames: u64,
        dropped_frames: u64,
        persisted_frames: u64,
        gap_count: u64,
    ) -> Result<Self> {
        let statistics = Self {
            received_frames,
            acknowledged_frames,
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

    pub const fn acknowledged_frames(&self) -> u64 {
        self.acknowledged_frames
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

    pub fn record_received(mut self) -> Result<Self> {
        self.received_frames = self.received_frames.saturating_add(1);
        self.validate()
    }

    pub fn record_acknowledged(mut self) -> Result<Self> {
        self.acknowledged_frames = self.acknowledged_frames.saturating_add(1);
        self.validate()
    }

    pub fn record_accepted(mut self) -> Result<Self> {
        self.accepted_frames = self.accepted_frames.saturating_add(1);
        self.validate()
    }

    pub fn record_dropped(mut self) -> Result<Self> {
        self.dropped_frames = self.dropped_frames.saturating_add(1);
        self.validate()
    }

    pub fn record_persisted(mut self) -> Result<Self> {
        self.persisted_frames = self.persisted_frames.saturating_add(1);
        self.validate()
    }

    pub fn record_gap(mut self) -> Result<Self> {
        self.gap_count = self.gap_count.saturating_add(1);
        self.validate()
    }

    /// Replace all counters atomically, leaving the previous valid value intact on error.
    pub fn update(
        &mut self,
        received_frames: u64,
        acknowledged_frames: u64,
        accepted_frames: u64,
        dropped_frames: u64,
        persisted_frames: u64,
        gap_count: u64,
    ) -> Result<()> {
        *self = Self::new(
            received_frames,
            acknowledged_frames,
            accepted_frames,
            dropped_frames,
            persisted_frames,
            gap_count,
        )?;
        Ok(())
    }

    pub fn validate(self) -> Result<Self> {
        if self.acknowledged_frames > self.received_frames {
            return Err(invalid("acknowledged frames exceed received frames"));
        }
        let accounted = self
            .accepted_frames
            .checked_add(self.dropped_frames)
            .ok_or_else(|| invalid("capture frame statistics overflow"))?;
        if accounted > self.acknowledged_frames {
            return Err(invalid(
                "accepted and dropped frames exceed acknowledged frames",
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
                wire.acknowledged_frames,
                wire.accepted_frames,
                wire.dropped_frames,
                wire.persisted_frames,
                wire.gap_count,
            )
        })
    }
}

define_stable_enum! {
    pub enum CaptureStreamState {
        Starting => "starting",
        Capturing => "capturing",
        PausedBudget => "paused_budget",
        Hidden => "hidden",
        Suspended => "suspended",
        Draining => "draining",
        Stopped => "stopped",
        Failed => "failed",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureFailure {
    stage: CaptureFailureStage,
    cause: KrometrailError,
}

impl CaptureFailure {
    pub fn new(stage: CaptureFailureStage, cause: KrometrailError) -> Result<Self> {
        if cause.code != crate::ErrorCode::CaptureFailed
            && cause.code != crate::ErrorCode::PersistenceFailed
        {
            return Err(invalid(
                "capture failure cause must be capture_failed or persistence_failed",
            ));
        }
        Ok(Self { stage, cause })
    }

    pub const fn stage(&self) -> CaptureFailureStage {
        self.stage
    }

    pub const fn cause(&self) -> &KrometrailError {
        &self.cause
    }
}

define_stable_enum! {
    /// Sanitized terminal boundary at which retained visual capture stopped.
    pub enum CaptureFailureStage {
        FrameEventStream => "frame_event_stream",
        VisibilityEventStream => "visibility_event_stream",
        FrameEnvelope => "frame_envelope",
        Acknowledgement => "acknowledgement",
        OrdinalAllocation => "ordinal_allocation",
        FrameDecode => "frame_decode",
        FramePersistence => "frame_persistence",
        GapPersistence => "gap_persistence",
    }
}

/// Fixed-bucket percentiles are upper bounds; unlike them, `max_nanos` is exact and may be
/// lower than a percentile bucket bound.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureTimingSummary {
    sample_count: u64,
    p50_nanos: Option<u64>,
    p95_nanos: Option<u64>,
    p99_nanos: Option<u64>,
    max_nanos: Option<u64>,
}

#[derive(Deserialize)]
struct CaptureTimingSummaryWire {
    sample_count: u64,
    p50_nanos: Option<u64>,
    p95_nanos: Option<u64>,
    p99_nanos: Option<u64>,
    max_nanos: Option<u64>,
}

impl CaptureTimingSummary {
    pub fn new(
        sample_count: u64,
        p50_nanos: Option<u64>,
        p95_nanos: Option<u64>,
        p99_nanos: Option<u64>,
        max_nanos: Option<u64>,
    ) -> Result<Self> {
        let summary = Self {
            sample_count,
            p50_nanos,
            p95_nanos,
            p99_nanos,
            max_nanos,
        };
        if sample_count == 0 {
            if p50_nanos.is_some()
                || p95_nanos.is_some()
                || p99_nanos.is_some()
                || max_nanos.is_some()
            {
                return Err(invalid(
                    "empty timing summaries cannot contain measurements",
                ));
            }
        } else if [p50_nanos, p95_nanos, p99_nanos, max_nanos]
            .iter()
            .any(Option::is_none)
        {
            return Err(invalid(
                "non-empty timing summaries require all measurements",
            ));
        } else if !(p50_nanos.unwrap() <= p95_nanos.unwrap()
            && p95_nanos.unwrap() <= p99_nanos.unwrap())
        {
            return Err(invalid("timing summary percentiles are not ordered"));
        }
        Ok(summary)
    }

    pub const fn empty() -> Self {
        Self {
            sample_count: 0,
            p50_nanos: None,
            p95_nanos: None,
            p99_nanos: None,
            max_nanos: None,
        }
    }

    pub const fn sample_count(&self) -> u64 {
        self.sample_count
    }

    pub const fn p50_nanos(&self) -> Option<u64> {
        self.p50_nanos
    }

    pub const fn p95_nanos(&self) -> Option<u64> {
        self.p95_nanos
    }

    pub const fn p99_nanos(&self) -> Option<u64> {
        self.p99_nanos
    }

    pub const fn max_nanos(&self) -> Option<u64> {
        self.max_nanos
    }
}

impl<'de> Deserialize<'de> for CaptureTimingSummary {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: CaptureTimingSummaryWire| {
            Self::new(
                wire.sample_count,
                wire.p50_nanos,
                wire.p95_nanos,
                wire.p99_nanos,
                wire.max_nanos,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetCaptureStatus {
    target_id: crate::ids::TargetId,
    attachment_generation: u64,
    state: CaptureStreamState,
    statistics: CaptureStatistics,
    queue_capacity: usize,
    queue_depth: usize,
    last_frame_session_time: Option<crate::time::SessionTime>,
    ack_latency: CaptureTimingSummary,
    frame_cadence: CaptureTimingSummary,
    every_nth_frame: EveryNthFrame,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<CaptureFailure>,
}

#[derive(Deserialize)]
struct TargetCaptureStatusWire {
    target_id: crate::ids::TargetId,
    attachment_generation: u64,
    state: CaptureStreamState,
    statistics: CaptureStatistics,
    queue_capacity: usize,
    queue_depth: usize,
    last_frame_session_time: Option<crate::time::SessionTime>,
    ack_latency: CaptureTimingSummary,
    frame_cadence: CaptureTimingSummary,
    every_nth_frame: EveryNthFrame,
    #[serde(default)]
    failure: Option<CaptureFailure>,
}

impl TargetCaptureStatus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_id: crate::ids::TargetId,
        attachment_generation: u64,
        state: CaptureStreamState,
        statistics: CaptureStatistics,
        queue_capacity: usize,
        queue_depth: usize,
        last_frame_session_time: Option<crate::time::SessionTime>,
        ack_latency: CaptureTimingSummary,
        frame_cadence: CaptureTimingSummary,
        every_nth_frame: EveryNthFrame,
        failure: Option<CaptureFailure>,
    ) -> Result<Self> {
        if attachment_generation == 0 {
            return Err(invalid("capture attachment generation must be non-zero"));
        }
        if queue_capacity == 0 {
            return Err(invalid("capture queue capacity must be non-zero"));
        }
        if queue_depth > queue_capacity {
            return Err(invalid("capture queue depth exceeds capacity"));
        }
        if matches!(state, CaptureStreamState::Stopped) && queue_depth != 0 {
            return Err(invalid(
                "stopped capture streams cannot retain queued frames",
            ));
        }
        if matches!(state, CaptureStreamState::Failed) != failure.is_some() {
            return Err(invalid(
                "failed capture streams require exactly one failure",
            ));
        }
        if last_frame_session_time.is_some() && statistics.received_frames() == 0 {
            return Err(invalid(
                "last frame time requires at least one received frame",
            ));
        }
        Ok(Self {
            target_id,
            attachment_generation,
            state,
            statistics,
            queue_capacity,
            queue_depth,
            last_frame_session_time,
            ack_latency,
            frame_cadence,
            every_nth_frame,
            failure,
        })
    }

    pub const fn target_id(&self) -> crate::ids::TargetId {
        self.target_id
    }

    pub const fn attachment_generation(&self) -> u64 {
        self.attachment_generation
    }

    pub const fn state(&self) -> CaptureStreamState {
        self.state
    }

    pub const fn statistics(&self) -> &CaptureStatistics {
        &self.statistics
    }

    pub const fn queue_capacity(&self) -> usize {
        self.queue_capacity
    }

    pub const fn queue_depth(&self) -> usize {
        self.queue_depth
    }

    pub const fn last_frame_session_time(&self) -> Option<crate::time::SessionTime> {
        self.last_frame_session_time
    }

    pub const fn ack_latency(&self) -> &CaptureTimingSummary {
        &self.ack_latency
    }

    pub const fn frame_cadence(&self) -> &CaptureTimingSummary {
        &self.frame_cadence
    }

    pub const fn every_nth_frame(&self) -> EveryNthFrame {
        self.every_nth_frame
    }

    pub const fn failure(&self) -> Option<&CaptureFailure> {
        self.failure.as_ref()
    }
}

impl<'de> Deserialize<'de> for TargetCaptureStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: TargetCaptureStatusWire| {
            Self::new(
                wire.target_id,
                wire.attachment_generation,
                wire.state,
                wire.statistics,
                wire.queue_capacity,
                wire.queue_depth,
                wire.last_frame_session_time,
                wire.ack_latency,
                wire.frame_cadence,
                wire.every_nth_frame,
                wire.failure,
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
    profile: ProfileRef,
    lifecycle: SessionLifecycle,
    disk_budget: DiskBudgetBytes,
    capabilities: Vec<CapabilityId>,
    statistics: CaptureStatistics,
    every_nth_frame: EveryNthFrame,
}

#[derive(Deserialize)]
struct RecordingSessionWire {
    id: SessionId,
    origin: ObservedTime,
    started_at: SystemTime,
    ended_at: Option<SystemTime>,
    browser: BrowserVersion,
    profile: ProfileRef,
    lifecycle: SessionLifecycle,
    disk_budget: DiskBudgetBytes,
    capabilities: Vec<CapabilityId>,
    statistics: CaptureStatistics,
    every_nth_frame: EveryNthFrame,
}

impl RecordingSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SessionId,
        origin: ObservedTime,
        started_at: SystemTime,
        browser: BrowserVersion,
        profile: ProfileRef,
        disk_budget: DiskBudgetBytes,
        capabilities: Vec<CapabilityId>,
        every_nth_frame: EveryNthFrame,
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
            every_nth_frame,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        id: SessionId,
        origin: ObservedTime,
        started_at: SystemTime,
        ended_at: Option<SystemTime>,
        browser: BrowserVersion,
        profile: ProfileRef,
        lifecycle: SessionLifecycle,
        disk_budget: DiskBudgetBytes,
        capabilities: Vec<CapabilityId>,
        statistics: CaptureStatistics,
        every_nth_frame: EveryNthFrame,
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
            every_nth_frame,
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

    pub fn profile(&self) -> &ProfileRef {
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

    pub const fn every_nth_frame(&self) -> EveryNthFrame {
        self.every_nth_frame
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
                wire.every_nth_frame,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{browser::BrowserVersion, ids::SessionId, time::SessionTime};

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn session() -> RecordingSession {
        RecordingSession::new(
            SessionId::from_uuid(UUID.parse().unwrap()),
            ObservedTime::from_nanos(1),
            SystemTime::UNIX_EPOCH,
            BrowserVersion::new(
                crate::BrowserProduct::Chrome,
                crate::BrowserProductVersion::new("128").unwrap(),
                "revision",
                "1.3",
                "Chrome/128",
                "12",
            )
            .unwrap(),
            ProfileRef::managed(crate::ProfileIdentity::new("profile").unwrap()),
            DiskBudgetBytes::new(1024).unwrap(),
            vec![CapabilityId::Control],
            EveryNthFrame::default(),
        )
        .unwrap()
    }

    #[test]
    fn managed_and_external_profiles_round_trip_in_recording_sessions() {
        let attached = RecordingSession::new(
            SessionId::from_uuid(UUID.parse().unwrap()),
            ObservedTime::from_nanos(1),
            SystemTime::UNIX_EPOCH,
            BrowserVersion::new(
                crate::BrowserProduct::Chromium,
                crate::BrowserProductVersion::new("128").unwrap(),
                "revision",
                "1.3",
                "Chromium/128",
                "12",
            )
            .unwrap(),
            ProfileRef::External,
            DiskBudgetBytes::new(1024).unwrap(),
            vec![CapabilityId::Control],
            EveryNthFrame::default(),
        )
        .unwrap();
        let encoded = serde_json::to_string(&attached).unwrap();
        let decoded = serde_json::from_str::<RecordingSession>(&encoded).unwrap();
        assert_eq!(decoded.profile(), &ProfileRef::External);
        assert_eq!(decoded, attached);
    }

    #[test]
    fn rejects_zero_budget_and_inconsistent_statistics() {
        assert!(DiskBudgetBytes::new(0).is_err());
        assert!(CaptureStatistics::new(1, 1, 1, 1, 0, 0).is_err());
        assert!(CaptureStatistics::new(1, 1, 0, 0, 2, 0).is_err());
        assert!(CaptureStatistics::new(0, 1, 0, 0, 0, 0).is_err());
        assert!(CaptureStatistics::new(u64::MAX, u64::MAX, u64::MAX, 1, 0, 0).is_err());
    }

    #[test]
    fn statistics_are_readable_and_mutated_atomically() {
        let mut statistics = CaptureStatistics::new(2, 2, 1, 1, 1, 0).unwrap();
        assert_eq!(statistics.received_frames(), 2);
        assert_eq!(statistics.acknowledged_frames(), 2);
        assert!(statistics.update(1, 1, 1, 1, 0, 0).is_err());
        assert_eq!(statistics.received_frames(), 2);
        statistics.update(3, 3, 2, 1, 2, 1).unwrap();
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
    fn validates_timing_and_target_status_boundaries() {
        let target = crate::ids::TargetId::from_uuid(UUID.parse().unwrap());
        let empty = CaptureTimingSummary::empty();
        assert!(CaptureTimingSummary::new(1, Some(1), None, Some(1), Some(1)).is_err());
        assert!(
            TargetCaptureStatus::new(
                target,
                0,
                CaptureStreamState::Capturing,
                CaptureStatistics::default(),
                1,
                0,
                None,
                empty.clone(),
                empty.clone(),
                EveryNthFrame::default(),
                None,
            )
            .is_err()
        );
        assert!(
            TargetCaptureStatus::new(
                target,
                1,
                CaptureStreamState::Failed,
                CaptureStatistics::default(),
                1,
                0,
                None,
                CaptureTimingSummary::empty(),
                CaptureTimingSummary::empty(),
                EveryNthFrame::default(),
                None,
            )
            .is_err()
        );
        assert!(
            TargetCaptureStatus::new(
                target,
                1,
                CaptureStreamState::Capturing,
                CaptureStatistics::default(),
                1,
                0,
                None,
                CaptureTimingSummary::empty(),
                CaptureTimingSummary::empty(),
                EveryNthFrame::default(),
                Some(
                    CaptureFailure::new(
                        CaptureFailureStage::FramePersistence,
                        KrometrailError::new(
                            crate::ErrorCode::PersistenceFailed,
                            crate::NonEmptyText::new("frame persistence failed").unwrap(),
                        ),
                    )
                    .unwrap()
                ),
            )
            .is_err()
        );
        let expected_failure = CaptureFailure::new(
            CaptureFailureStage::FramePersistence,
            KrometrailError::new(
                crate::ErrorCode::PersistenceFailed,
                crate::NonEmptyText::new("frame persistence failed").unwrap(),
            ),
        )
        .unwrap();
        let failed = TargetCaptureStatus::new(
            target,
            1,
            CaptureStreamState::Failed,
            CaptureStatistics::default(),
            1,
            0,
            None,
            CaptureTimingSummary::empty(),
            CaptureTimingSummary::empty(),
            EveryNthFrame::default(),
            Some(expected_failure.clone()),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<TargetCaptureStatus>(&serde_json::to_string(&failed).unwrap())
                .unwrap()
                .failure(),
            Some(&expected_failure)
        );
        assert!(
            TargetCaptureStatus::new(
                target,
                1,
                CaptureStreamState::Capturing,
                CaptureStatistics::default(),
                0,
                0,
                None,
                empty.clone(),
                empty.clone(),
                EveryNthFrame::default(),
                None,
            )
            .is_err()
        );
        assert!(
            TargetCaptureStatus::new(
                target,
                1,
                CaptureStreamState::Capturing,
                CaptureStatistics::default(),
                1,
                2,
                None,
                empty.clone(),
                empty.clone(),
                EveryNthFrame::default(),
                None,
            )
            .is_err()
        );
        assert!(
            TargetCaptureStatus::new(
                target,
                1,
                CaptureStreamState::Capturing,
                CaptureStatistics::default(),
                1,
                0,
                Some(SessionTime::ZERO),
                empty.clone(),
                empty.clone(),
                EveryNthFrame::default(),
                None,
            )
            .is_err()
        );
        assert!(
            TargetCaptureStatus::new(
                target,
                1,
                CaptureStreamState::Stopped,
                CaptureStatistics::default(),
                1,
                1,
                None,
                empty.clone(),
                empty,
                EveryNthFrame::default(),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn status_round_trips_requested_stride_without_changing_statistics() {
        let target = crate::ids::TargetId::from_uuid(UUID.parse().unwrap());
        let status = TargetCaptureStatus::new(
            target,
            1,
            CaptureStreamState::Capturing,
            CaptureStatistics::new(2, 2, 2, 0, 2, 0).unwrap(),
            4,
            0,
            Some(SessionTime::from_nanos(2)),
            CaptureTimingSummary::empty(),
            CaptureTimingSummary::empty(),
            EveryNthFrame::new(23).unwrap(),
            None,
        )
        .unwrap();
        let encoded = serde_json::to_string(&status).unwrap();
        assert!(encoded.contains("every_nth_frame"));
        let decoded = serde_json::from_str::<TargetCaptureStatus>(&encoded).unwrap();
        assert_eq!(decoded, status);
        assert_eq!(decoded.every_nth_frame().get(), 23);
        assert_eq!(decoded.statistics().accepted_frames(), 2);
    }

    #[test]
    fn recording_session_round_trips_requested_stride() {
        let mut value = serde_json::to_value(session()).unwrap();
        value["every_nth_frame"] = serde_json::json!(41);
        let decoded = serde_json::from_value::<RecordingSession>(value).unwrap();
        assert_eq!(decoded.every_nth_frame().get(), 41);
        assert_eq!(decoded.statistics(), &CaptureStatistics::default());
    }

    #[test]
    fn rejects_malformed_serialized_statistics_and_sessions() {
        let malformed_statistics = r#"{"received_frames":1,"acknowledged_frames":1,"accepted_frames":1,"dropped_frames":1,"persisted_frames":0,"gap_count":0}"#;
        assert!(serde_json::from_str::<CaptureStatistics>(malformed_statistics).is_err());
        let malformed_session = format!(
            r#"{{"id":"{UUID}","origin":1,"started_at":{{"secs_since_epoch":0,"nanos_since_epoch":0}},"ended_at":null,"browser":{{"product":"Chrome","product_version":"128","revision":"r","protocol_version":"1.3","user_agent":"Chrome/128","js_version":"12"}},"profile":"profile","lifecycle":"ended","disk_budget":1024,"capabilities":["control"],"statistics":{{"received_frames":0,"acknowledged_frames":0,"accepted_frames":0,"dropped_frames":0,"persisted_frames":0,"gap_count":0}}}}"#
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
