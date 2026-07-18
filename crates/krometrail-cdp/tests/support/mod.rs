#![allow(dead_code)]

pub mod cdp_proxy;
pub mod chrome;
pub mod retention;
pub mod scripted_cdp;
pub mod smoke_evidence;
pub mod static_fixture;

use std::sync::{Arc, Mutex};

use krometrail_core::{
    InteractionAnchor, InteractionEvidenceSink, InteractionRecord, NavigationId, ObservedTime,
    PortFuture,
};

type EvidenceEntry = (
    InteractionAnchor,
    Option<InteractionRecord>,
    ObservedTime,
    Option<NavigationId>,
);

#[derive(Default)]
pub struct RecordingEvidenceFake {
    entries: Mutex<Vec<EvidenceEntry>>,
}

impl InteractionEvidenceSink for RecordingEvidenceFake {
    fn append_operation_evidence(
        &self,
        anchor: InteractionAnchor,
        record: Option<InteractionRecord>,
        persisted_at: ObservedTime,
        navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.entries.lock().expect("evidence fake lock").push((
            anchor,
            record,
            persisted_at,
            navigation_id,
        ));
        Box::pin(std::future::ready(Ok(())))
    }
}

impl RecordingEvidenceFake {
    pub fn records(&self) -> Vec<InteractionRecord> {
        self.entries
            .lock()
            .expect("evidence fake lock")
            .iter()
            .filter_map(|(_, record, _, _)| record.clone())
            .collect()
    }
}

pub fn evidence_sink() -> Arc<dyn InteractionEvidenceSink> {
    Arc::new(RecordingEvidenceFake::default())
}
