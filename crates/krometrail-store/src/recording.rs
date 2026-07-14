use std::sync::Arc;

use krometrail_core::{
    CaptureGap, CaptureGapStore, EncodedFrame, FrameAddress, PortFuture, RecordingSink, SessionId,
};
use tokio::sync::Mutex;

use crate::index::{frames::index_frame_tx, segments::register_segment_tx};
use crate::{SegmentWriter, SqliteIndex, persistence_error};

/// Orders complete segment mutations before their searchable metadata commits.
pub struct IndexedRecordingSink {
    mutations: Mutex<()>,
    segments: Arc<SegmentWriter>,
    index: Arc<SqliteIndex>,
}

impl IndexedRecordingSink {
    pub fn new(segments: Arc<SegmentWriter>, index: Arc<SqliteIndex>) -> Self {
        Self {
            mutations: Mutex::new(()),
            segments,
            index,
        }
    }
}

impl RecordingSink for IndexedRecordingSink {
    fn append_frame(
        &self,
        frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            let commit = self.segments.append_indexable(frame.clone()).await?;
            let mut connection = self.index.connection()?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin indexed frame persistence"))?;
            index_frame_tx(&transaction, &frame, &commit)?;
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit indexed frame metadata"))?;
            Ok(commit.address)
        })
    }

    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            CaptureGapStore::append_gap(self.index.as_ref(), gap).await
        })
    }

    fn flush(&self, session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            let registrations = self.segments.flush_indexable(session_id).await?;
            let mut connection = self.index.connection()?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin indexed recording flush"))?;
            for registration in &registrations {
                register_segment_tx(&transaction, registration)?;
            }
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit indexed recording flush"))
        })
    }
}
