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
        if let (Some(oldest), Some(newest)) = (oldest_retained, newest_retained)
            && oldest.session_id == newest.session_id
            && oldest.target_id == newest.target_id
            && oldest.session_time > newest.session_time
        {
            return Err(invalid("retained bounds are not ordered"));
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
