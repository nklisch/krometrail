use std::{
    num::{NonZeroU16, NonZeroU64},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventId, BrowserEventOrdinal,
    BrowserEventSeverity, SessionId, SessionRange, SessionTime, TargetId,
    error::{Result, invalid},
    validation::deserialize_validated,
};

use super::PortFuture;

pub const DEFAULT_EVENT_PAGE_ROWS: u16 = 100;
pub const MAX_EVENT_PAGE_ROWS: u16 = 1_000;
pub const MAX_EVENT_CANDIDATE_ROWS: u16 = 256;
pub const MAX_EVENT_UNAVAILABLE_RANGES: u16 = 1_000;
pub const MAX_CAPTURE_STATUS_SAMPLES: u16 = 128;

/// Durable append boundary for normalized browser events.
///
/// Implementations receive only validated, privacy-safe batches. The port is
/// runtime neutral and object safe so persistence cannot leak inward or select
/// the core executor.
pub trait BrowserEventSink: Send + Sync {
    fn append_event_batch(&self, batch: BrowserEventBatch) -> PortFuture<'_, Result<()>>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserEventSelector {
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    classes: Vec<BrowserEventClass>,
    minimum_severity: BrowserEventSeverity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventSelectorWire {
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    classes: Vec<BrowserEventClass>,
    minimum_severity: BrowserEventSeverity,
}

impl BrowserEventSelector {
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        mut classes: Vec<BrowserEventClass>,
        minimum_severity: BrowserEventSeverity,
    ) -> Result<Self> {
        if session_id.as_uuid().is_nil() || target_id.as_uuid().is_nil() {
            return Err(invalid("browser event selector scope IDs must not be nil"));
        }
        classes.sort_unstable();
        if classes.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid("browser event selector classes must be unique"));
        }
        Ok(Self {
            session_id,
            target_id,
            range,
            classes,
            minimum_severity,
        })
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

    /// An empty class set selects every registered browser-event class.
    pub fn classes(&self) -> &[BrowserEventClass] {
        &self.classes
    }

    pub const fn minimum_severity(&self) -> BrowserEventSeverity {
        self.minimum_severity
    }
}

impl<'de> Deserialize<'de> for BrowserEventSelector {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventSelectorWire| {
            Self::new(
                wire.session_id,
                wire.target_id,
                wire.range,
                wire.classes,
                wire.minimum_severity,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserEventCursor {
    selector: BrowserEventSelector,
    session_time: SessionTime,
    ordinal: BrowserEventOrdinal,
    event_id: BrowserEventId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventCursorWire {
    selector: BrowserEventSelector,
    session_time: SessionTime,
    ordinal: BrowserEventOrdinal,
    event_id: BrowserEventId,
}

impl BrowserEventCursor {
    pub fn new(
        selector: BrowserEventSelector,
        session_time: SessionTime,
        ordinal: BrowserEventOrdinal,
        event_id: BrowserEventId,
    ) -> Result<Self> {
        if !selector.range().contains(session_time) || event_id.as_uuid().is_nil() {
            return Err(invalid("browser event cursor is outside its selector"));
        }
        Ok(Self {
            selector,
            session_time,
            ordinal,
            event_id,
        })
    }

    pub fn selector(&self) -> &BrowserEventSelector {
        &self.selector
    }

    pub const fn session_time(&self) -> SessionTime {
        self.session_time
    }

    pub const fn ordinal(&self) -> BrowserEventOrdinal {
        self.ordinal
    }

    pub const fn event_id(&self) -> BrowserEventId {
        self.event_id
    }
}

impl<'de> Deserialize<'de> for BrowserEventCursor {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventCursorWire| {
            Self::new(
                wire.selector,
                wire.session_time,
                wire.ordinal,
                wire.event_id,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct EventPageLimit(NonZeroU16);

impl EventPageLimit {
    pub fn new(value: u16) -> Result<Self> {
        let value = NonZeroU16::new(value)
            .filter(|value| value.get() <= MAX_EVENT_PAGE_ROWS)
            .ok_or_else(|| invalid("browser event page limit is out of range"))?;
        Ok(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Default for EventPageLimit {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_PAGE_ROWS).expect("default browser event page limit is valid")
    }
}

impl<'de> Deserialize<'de> for EventPageLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, Self::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct EventCandidateLimit(NonZeroU16);

impl EventCandidateLimit {
    pub fn new(value: u16) -> Result<Self> {
        let value = NonZeroU16::new(value)
            .filter(|value| value.get() <= MAX_EVENT_CANDIDATE_ROWS)
            .ok_or_else(|| invalid("browser event candidate limit is out of range"))?;
        Ok(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for EventCandidateLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, Self::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEventUnavailableReason {
    RetentionEvicted,
    CorruptDiscarded,
}

impl BrowserEventUnavailableReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RetentionEvicted => "retention_evicted",
            Self::CorruptDiscarded => "corrupt_discarded",
        }
    }

    pub fn from_stable_name(value: &str) -> Option<Self> {
        match value {
            "retention_evicted" => Some(Self::RetentionEvicted),
            "corrupt_discarded" => Some(Self::CorruptDiscarded),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserEventUnavailableRange {
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    first_ordinal: Option<BrowserEventOrdinal>,
    last_ordinal: Option<BrowserEventOrdinal>,
    event_count: NonZeroU64,
    reason: BrowserEventUnavailableReason,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventUnavailableRangeWire {
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    first_ordinal: Option<BrowserEventOrdinal>,
    last_ordinal: Option<BrowserEventOrdinal>,
    event_count: NonZeroU64,
    reason: BrowserEventUnavailableReason,
}

impl BrowserEventUnavailableRange {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        first_ordinal: Option<BrowserEventOrdinal>,
        last_ordinal: Option<BrowserEventOrdinal>,
        event_count: NonZeroU64,
        reason: BrowserEventUnavailableReason,
    ) -> Result<Self> {
        if session_id.as_uuid().is_nil() || target_id.as_uuid().is_nil() {
            return Err(invalid(
                "browser event unavailable scope IDs must not be nil",
            ));
        }
        if first_ordinal.is_some() != last_ordinal.is_some()
            || first_ordinal
                .zip(last_ordinal)
                .is_some_and(|(first, last)| first > last)
        {
            return Err(invalid(
                "browser event unavailable ordinal range is invalid",
            ));
        }
        Ok(Self {
            session_id,
            target_id,
            range,
            first_ordinal,
            last_ordinal,
            event_count,
            reason,
        })
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
    pub const fn first_ordinal(&self) -> Option<BrowserEventOrdinal> {
        self.first_ordinal
    }
    pub const fn last_ordinal(&self) -> Option<BrowserEventOrdinal> {
        self.last_ordinal
    }
    pub const fn event_count(&self) -> NonZeroU64 {
        self.event_count
    }
    pub const fn reason(&self) -> BrowserEventUnavailableReason {
        self.reason
    }
}

impl<'de> Deserialize<'de> for BrowserEventUnavailableRange {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventUnavailableRangeWire| {
            Self::new(
                wire.session_id,
                wire.target_id,
                wire.range,
                wire.first_ordinal,
                wire.last_ordinal,
                wire.event_count,
                wire.reason,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureStatusSamples {
    at_or_before: Option<BrowserEvent>,
    in_range: Vec<BrowserEvent>,
}

impl CaptureStatusSamples {
    pub fn new(at_or_before: Option<BrowserEvent>, in_range: Vec<BrowserEvent>) -> Result<Self> {
        if in_range.len() > usize::from(MAX_CAPTURE_STATUS_SAMPLES)
            || at_or_before
                .iter()
                .chain(&in_range)
                .any(|event| event.kind() != crate::BrowserEventKind::CaptureStatusChanged)
        {
            return Err(invalid("capture status samples are invalid"));
        }
        Ok(Self {
            at_or_before,
            in_range,
        })
    }

    pub const fn at_or_before(&self) -> Option<&BrowserEvent> {
        self.at_or_before.as_ref()
    }

    pub fn in_range(&self) -> &[BrowserEvent] {
        &self.in_range
    }
}

/// Bounded semantic reads over retained browser-event evidence.
pub trait BrowserEventSource: Send + Sync {
    fn count_events(&self, selector: BrowserEventSelector) -> PortFuture<'_, Result<u64>>;

    fn chronological_events(
        &self,
        selector: BrowserEventSelector,
        cursor: Option<BrowserEventCursor>,
        limit: EventPageLimit,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>>;

    fn priority_candidates(
        &self,
        selector: BrowserEventSelector,
        limit: EventCandidateLimit,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>>;

    fn nearest_candidates(
        &self,
        selector: BrowserEventSelector,
        focus_times: Vec<SessionTime>,
        each_side: u8,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>>;

    fn unavailable_ranges(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, Result<Vec<BrowserEventUnavailableRange>>>;

    fn capture_status_samples(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, Result<CaptureStatusSamples>>;
}

impl<T: BrowserEventSource + ?Sized> BrowserEventSource for Arc<T> {
    fn count_events(&self, selector: BrowserEventSelector) -> PortFuture<'_, Result<u64>> {
        (**self).count_events(selector)
    }

    fn chronological_events(
        &self,
        selector: BrowserEventSelector,
        cursor: Option<BrowserEventCursor>,
        limit: EventPageLimit,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>> {
        (**self).chronological_events(selector, cursor, limit)
    }

    fn priority_candidates(
        &self,
        selector: BrowserEventSelector,
        limit: EventCandidateLimit,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>> {
        (**self).priority_candidates(selector, limit)
    }

    fn nearest_candidates(
        &self,
        selector: BrowserEventSelector,
        focus_times: Vec<SessionTime>,
        each_side: u8,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>> {
        (**self).nearest_candidates(selector, focus_times, each_side)
    }

    fn unavailable_ranges(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, Result<Vec<BrowserEventUnavailableRange>>> {
        (**self).unavailable_ranges(session_id, target_id, range, limit)
    }

    fn capture_status_samples(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, Result<CaptureStatusSamples>> {
        (**self).capture_status_samples(session_id, target_id, range, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn selector() -> BrowserEventSelector {
        BrowserEventSelector::new(
            SessionId::from_uuid(Uuid::from_u128(1)),
            TargetId::from_uuid(Uuid::from_u128(2)),
            SessionRange::new(SessionTime::from_nanos(3), SessionTime::from_nanos(5)).unwrap(),
            vec![BrowserEventClass::Network, BrowserEventClass::Console],
            BrowserEventSeverity::Info,
        )
        .unwrap()
    }

    #[test]
    fn semantic_read_contracts_are_bounded_and_scope_cursors() {
        let selector = selector();
        assert_eq!(
            selector.classes(),
            &[BrowserEventClass::Console, BrowserEventClass::Network]
        );
        assert!(EventPageLimit::new(0).is_err());
        assert!(EventPageLimit::new(MAX_EVENT_PAGE_ROWS + 1).is_err());
        assert!(EventCandidateLimit::new(MAX_EVENT_CANDIDATE_ROWS + 1).is_err());
        assert!(
            BrowserEventCursor::new(
                selector,
                SessionTime::from_nanos(6),
                BrowserEventOrdinal::new(1).unwrap(),
                BrowserEventId::from_uuid(Uuid::from_u128(4)),
            )
            .is_err()
        );
        let unavailable = BrowserEventUnavailableRange::new(
            SessionId::from_uuid(Uuid::from_u128(1)),
            TargetId::from_uuid(Uuid::from_u128(2)),
            SessionRange::new(SessionTime::from_nanos(3), SessionTime::from_nanos(4)).unwrap(),
            Some(BrowserEventOrdinal::new(5).unwrap()),
            Some(BrowserEventOrdinal::new(6).unwrap()),
            NonZeroU64::new(2).unwrap(),
            BrowserEventUnavailableReason::RetentionEvicted,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<BrowserEventUnavailableRange>(
                &serde_json::to_string(&unavailable).unwrap()
            )
            .unwrap(),
            unavailable
        );
    }

    #[test]
    fn browser_event_ports_are_object_safe() {
        fn accepts_sink(_: Option<&dyn BrowserEventSink>) {}
        fn accepts_source(_: Option<&dyn BrowserEventSource>) {}
        accepts_sink(None);
        accepts_source(None);
    }
}
