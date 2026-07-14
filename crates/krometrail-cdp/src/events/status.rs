use krometrail_core::{BrowserEventClass, BrowserEventCollectionStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEventStatus {
    pub state: BrowserEventCollectionStatus,
    pub unavailable_classes: Vec<BrowserEventClass>,
    pub dropped_count: u64,
    pub persisted_count: u64,
    pub pending_bytes: usize,
    pub pending_gap_count: usize,
}
