use std::{num::NonZeroU16, sync::Arc};

use crate::{
    error::{Result, invalid},
    ids::{SessionId, TargetId},
    time::SessionRange,
    timeline::{ObservationKind, TimelineObservation},
};

use super::PortFuture;

/// Maximum number of rows one bounded kind-filtered timeline read may return.
///
/// The bound keeps marker evidence compact; callers that need broader coverage
/// should add cursor-based progressive retrieval rather than raising this cap.
pub const MAX_TIMELINE_RANGE_ROWS: u16 = 4096;

/// Bounded, kind-filtered timeline read used by composition layers that need a
/// compact, deterministic slice of generic timeline evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineRangeQuery {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub range: SessionRange,
    pub kinds: Vec<ObservationKind>,
    pub limit: NonZeroU16,
}

impl TimelineRangeQuery {
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        mut kinds: Vec<ObservationKind>,
        limit: NonZeroU16,
    ) -> Result<Self> {
        if kinds.is_empty() {
            return Err(invalid(
                "timeline range query must request at least one observation kind",
            ));
        }
        if limit.get() > MAX_TIMELINE_RANGE_ROWS {
            return Err(invalid(
                "timeline range query limit must not exceed 4096 rows",
            ));
        }
        kinds.sort_unstable_by_key(|kind| kind.as_str());
        if kinds.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid(
                "timeline range query observation kinds must be unique",
            ));
        }
        Ok(Self {
            session_id,
            target_id,
            range,
            kinds,
            limit,
        })
    }

    /// Stable wire names for the requested kinds, in deterministic order, used
    /// by adapters to build the SQL `IN` clause.
    pub fn kind_names(&self) -> Vec<&'static str> {
        self.kinds.iter().map(|kind| kind.as_str()).collect()
    }
}

/// Result of one bounded kind-filtered timeline read.
///
/// `matched_count` is the exact count of rows matching the filter (independent
/// of the limit); `observations` holds at most `limit` rows in deterministic
/// timeline order; `truncated` is `matched_count > observations.len()`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineRangeSlice {
    pub matched_count: u64,
    pub observations: Vec<TimelineObservation>,
    pub truncated: bool,
}

/// Indexes timeline metadata independently of encoded recording payloads.
pub trait TimelineStore: Send + Sync {
    fn append(&self, observation: TimelineObservation) -> PortFuture<'_, Result<()>>;
    /// Returns observations in the inclusive range using a deterministic adapter-defined order.
    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<TimelineObservation>>>;
    /// Returns a bounded, kind-filtered slice of timeline observations with the
    /// exact matched count and explicit truncation. High-volume kinds (such as
    /// browser events) are excluded by requesting only the desired kinds.
    fn selected_range(
        &self,
        query: TimelineRangeQuery,
    ) -> PortFuture<'_, Result<TimelineRangeSlice>>;
}

impl<T: TimelineStore + ?Sized> TimelineStore for Arc<T> {
    fn append(&self, observation: TimelineObservation) -> PortFuture<'_, Result<()>> {
        (**self).append(observation)
    }
    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<TimelineObservation>>> {
        (**self).range(session_id, target_id, range)
    }
    fn selected_range(
        &self,
        query: TimelineRangeQuery,
    ) -> PortFuture<'_, Result<TimelineRangeSlice>> {
        (**self).selected_range(query)
    }
}
