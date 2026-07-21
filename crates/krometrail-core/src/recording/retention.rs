use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    Result, SegmentId, SessionId, SessionRange, SessionTime, TargetId, error::invalid,
    recording::DiskBudgetBytes, validation::deserialize_validated,
};

/// Classed physical usage of the managed recording data directory.
///
/// `pending_deletion_bytes` and `open_segment_bytes` are subsets of the class
/// totals and are intentionally not added by [`StorageUsage::total_bytes`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct StorageUsage {
    pub segment_bytes: u64,
    pub index_bytes: u64,
    pub browser_event_bytes: u64,
    pub artifact_bytes: u64,
    pub pending_deletion_bytes: u64,
    pub open_segment_bytes: u64,
    pub accounting_slack_bytes: u64,
}

impl StorageUsage {
    pub fn new(
        segment_bytes: u64,
        index_bytes: u64,
        browser_event_bytes: u64,
        artifact_bytes: u64,
        pending_deletion_bytes: u64,
        open_segment_bytes: u64,
        accounting_slack_bytes: u64,
    ) -> Result<Self> {
        let usage = Self {
            segment_bytes,
            index_bytes,
            browser_event_bytes,
            artifact_bytes,
            pending_deletion_bytes,
            open_segment_bytes,
            accounting_slack_bytes,
        };
        usage.validate()?;
        Ok(usage)
    }

    pub fn total_bytes(&self) -> Result<u64> {
        self.segment_bytes
            .checked_add(self.index_bytes)
            .and_then(|value| value.checked_add(self.browser_event_bytes))
            .and_then(|value| value.checked_add(self.artifact_bytes))
            .ok_or_else(|| invalid("storage usage overflow"))
    }

    fn validate(&self) -> Result<()> {
        let total = self.total_bytes()?;
        if self.open_segment_bytes > self.segment_bytes {
            return Err(invalid("open segment bytes exceed segment usage"));
        }
        if self.pending_deletion_bytes > total {
            return Err(invalid("pending deletion bytes exceed total usage"));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for StorageUsage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            segment_bytes: u64,
            index_bytes: u64,
            browser_event_bytes: u64,
            artifact_bytes: u64,
            pending_deletion_bytes: u64,
            open_segment_bytes: u64,
            accounting_slack_bytes: u64,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(
                wire.segment_bytes,
                wire.index_bytes,
                wire.browser_event_bytes,
                wire.artifact_bytes,
                wire.pending_deletion_bytes,
                wire.open_segment_bytes,
                wire.accounting_slack_bytes,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionRange {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub range: SessionRange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinChange {
    pub request: RetentionRange,
    pub protected_segments: Vec<SegmentId>,
    pub pinned_usage_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingBudgetState {
    Available,
    PausedBudget,
}

/// Complete retention lifecycle for the managed recording directory.
///
/// Size pressure alone is not a lifecycle: a store that only reclaims at the
/// budget wall accumulates until it hits the wall and then sits there. This
/// bundles the three policies that together make evidence expire on time as
/// well as on size, and they deliberately share one reclaim walk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetentionLifecycle {
    budget: DiskBudgetBytes,
    max_age: Option<Duration>,
    trim_high_water_percent: u8,
    artifact_grace: Duration,
}

/// Evidence older than this expires even when the store is well inside budget.
pub const DEFAULT_RETENTION_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Trimming begins once usage crosses this share of the budget, so a long
/// session reclaims as it goes instead of degrading into permanent near-full
/// pressure and only reclaiming when a frame no longer fits.
pub const DEFAULT_TRIM_HIGH_WATER_PERCENT: u8 = 85;
/// A freshly published artifact is shielded from cascade eviction for this long,
/// so a returned resource link is not already dying when the agent receives it.
pub const DEFAULT_ARTIFACT_GRACE: Duration = Duration::from_secs(15 * 60);

impl RetentionLifecycle {
    pub fn new(
        budget: DiskBudgetBytes,
        max_age: Option<Duration>,
        trim_high_water_percent: u8,
        artifact_grace: Duration,
    ) -> Result<Self> {
        if trim_high_water_percent == 0 || trim_high_water_percent > 100 {
            return Err(invalid(
                "retention trim high-water percent must be between 1 and 100",
            ));
        }
        if max_age.is_some_and(|age| age.is_zero()) {
            return Err(invalid("retention max age must be greater than zero"));
        }
        Ok(Self {
            budget,
            max_age,
            trim_high_water_percent,
            artifact_grace,
        })
    }

    pub fn with_budget(budget: DiskBudgetBytes) -> Self {
        Self {
            budget,
            max_age: Some(DEFAULT_RETENTION_MAX_AGE),
            trim_high_water_percent: DEFAULT_TRIM_HIGH_WATER_PERCENT,
            artifact_grace: DEFAULT_ARTIFACT_GRACE,
        }
    }

    pub const fn budget(self) -> DiskBudgetBytes {
        self.budget
    }

    pub const fn max_age(self) -> Option<Duration> {
        self.max_age
    }

    pub const fn artifact_grace(self) -> Duration {
        self.artifact_grace
    }

    /// Usage at or above this triggers in-session trimming.
    ///
    /// Taken against the caller's *effective* allowance rather than the
    /// configured total, so an instance sharing a budget trims relative to what
    /// it may actually occupy.
    pub const fn trim_high_water_bytes(self, effective_budget: u64) -> u64 {
        (effective_budget / 100).saturating_mul(self.trim_high_water_percent as u64)
    }
}

impl Default for RetentionLifecycle {
    fn default() -> Self {
        Self::with_budget(DiskBudgetBytes::default())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetainedPoint {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub session_time: SessionTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetentionStatus {
    pub configured_budget: DiskBudgetBytes,
    pub usage: StorageUsage,
    pub pinned_usage_bytes: u64,
    pub oldest_retained: Option<RetainedPoint>,
    pub newest_retained: Option<RetainedPoint>,
    pub budget_state: RecordingBudgetState,
    pub eviction_blocked: bool,
    pub recording_blocked: bool,
    pub open_segment_count: u64,
    pub open_segment_overhead_bytes: u64,
    pub open_segment_overhead_limit_bytes: u64,
}

impl RetentionStatus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        configured_budget: DiskBudgetBytes,
        usage: StorageUsage,
        pinned_usage_bytes: u64,
        oldest_retained: Option<RetainedPoint>,
        newest_retained: Option<RetainedPoint>,
        budget_state: RecordingBudgetState,
        eviction_blocked: bool,
        recording_blocked: bool,
        open_segment_count: u64,
        open_segment_overhead_bytes: u64,
        open_segment_overhead_limit_bytes: u64,
    ) -> Result<Self> {
        usage.validate()?;
        let total = usage.total_bytes()?;
        if pinned_usage_bytes > total {
            return Err(invalid("pinned usage exceeds total usage"));
        }
        if (oldest_retained.is_some()) != (newest_retained.is_some()) {
            return Err(invalid("retained bounds must both be present or absent"));
        }
        if let (Some(oldest), Some(newest)) = (oldest_retained, newest_retained) {
            if oldest.session_id == newest.session_id
                && oldest.target_id == newest.target_id
                && oldest.session_time > newest.session_time
            {
                return Err(invalid("retained bounds are not ordered"));
            }
        }
        let paused = budget_state == RecordingBudgetState::PausedBudget;
        if eviction_blocked != paused || recording_blocked != paused {
            return Err(invalid("retention blocked flags do not match budget state"));
        }
        if open_segment_overhead_bytes > usage.open_segment_bytes {
            return Err(invalid("open segment overhead exceeds open segment usage"));
        }
        if (open_segment_count == 0) != (open_segment_overhead_bytes == 0) {
            return Err(invalid("open segment count and overhead disagree"));
        }
        if !paused {
            let overage = total.saturating_sub(configured_budget.get());
            if overage > open_segment_overhead_limit_bytes
                || open_segment_overhead_bytes > open_segment_overhead_limit_bytes
            {
                return Err(invalid(
                    "available retention status exceeds open-segment tolerance",
                ));
            }
        }
        Ok(Self {
            configured_budget,
            usage,
            pinned_usage_bytes,
            oldest_retained,
            newest_retained,
            budget_state,
            eviction_blocked,
            recording_blocked,
            open_segment_count,
            open_segment_overhead_bytes,
            open_segment_overhead_limit_bytes,
        })
    }

    pub fn empty(configured_budget: DiskBudgetBytes) -> Self {
        Self::new(
            configured_budget,
            StorageUsage::default(),
            0,
            None,
            None,
            RecordingBudgetState::Available,
            false,
            false,
            0,
            0,
            0,
        )
        .expect("empty retention status is valid")
    }
}

impl<'de> Deserialize<'de> for RetentionStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            configured_budget: DiskBudgetBytes,
            usage: StorageUsage,
            pinned_usage_bytes: u64,
            oldest_retained: Option<RetainedPoint>,
            newest_retained: Option<RetainedPoint>,
            budget_state: RecordingBudgetState,
            eviction_blocked: bool,
            recording_blocked: bool,
            open_segment_count: u64,
            open_segment_overhead_bytes: u64,
            open_segment_overhead_limit_bytes: u64,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(
                wire.configured_budget,
                wire.usage,
                wire.pinned_usage_bytes,
                wire.oldest_retained,
                wire.newest_retained,
                wire.budget_state,
                wire.eviction_blocked,
                wire.recording_blocked,
                wire.open_segment_count,
                wire.open_segment_overhead_bytes,
                wire.open_segment_overhead_limit_bytes,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionDeletion {
    pub session_id: SessionId,
    pub removed_segments: u64,
    pub removed_frames: u64,
    pub removed_artifacts: u64,
    pub removed_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn storage_usage_and_status_reject_impossible_boundaries() {
        assert!(StorageUsage::new(1, 0, 0, 0, 0, 2, 0).is_err());
        assert!(StorageUsage::new(u64::MAX, 1, 0, 0, 0, 0, 0).is_err());
        let budget = DiskBudgetBytes::new(10).unwrap();
        let usage = StorageUsage::new(11, 0, 0, 0, 0, 1, 0).unwrap();
        assert!(
            RetentionStatus::new(
                budget,
                usage,
                0,
                None,
                None,
                RecordingBudgetState::Available,
                false,
                false,
                1,
                1,
                0,
            )
            .is_err()
        );
        assert!(
            RetentionStatus::new(
                budget,
                usage,
                0,
                None,
                None,
                RecordingBudgetState::PausedBudget,
                false,
                false,
                1,
                1,
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn retention_values_round_trip_through_validated_serde() {
        let budget = DiskBudgetBytes::new(10).unwrap();
        let point = RetainedPoint {
            session_id: SessionId::from_uuid(Uuid::from_u128(1)),
            target_id: TargetId::from_uuid(Uuid::from_u128(2)),
            session_time: SessionTime::from_nanos(3),
        };
        let status = RetentionStatus::new(
            budget,
            StorageUsage::new(8, 1, 0, 0, 0, 0, 0).unwrap(),
            4,
            Some(point),
            Some(point),
            RecordingBudgetState::Available,
            false,
            false,
            0,
            0,
            0,
        )
        .unwrap();
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(
            serde_json::from_str::<RetentionStatus>(&json).unwrap(),
            status
        );
    }
}
