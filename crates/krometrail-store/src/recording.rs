use std::{
    collections::{BTreeSet, HashSet},
    sync::{Arc, Mutex as StdMutex},
};

use krometrail_core::{
    CaptureGap, CaptureGapStore, DiskBudgetBytes, EncodedFrame, ErrorCode, ErrorContext,
    FrameAddress, KrometrailError, NonEmptyText, PinChange, PortFuture, RecordingBudgetState,
    RecordingSink, RetentionRange, RetentionStatus, RetentionStore, RetryAdvice, SessionDeletion,
    SessionId, StorageUsage,
};
use tokio::sync::{Mutex, watch};

use crate::{
    SegmentRegistration, SegmentWriter, SqliteIndex,
    index::{
        deletion::{DeletionKind, DeletionObject, DeletionObjectKind, DeletionState},
        frames::index_frame_tx,
        retention::{ArtifactCandidate, SegmentCandidate},
        segments::register_segment_tx,
    },
    persistence_error,
    retention::removal::RemovalWorker,
};

const RECORD_ENVELOPE_ALLOWANCE: u64 = 4096;
const SQLITE_ACCOUNTING_SLACK: u64 = 32 * 1024;

/// Coordinates physical frame writes, searchable metadata, and destructive
/// retention mutations behind one ordering gate.
pub struct RecordingStore {
    mutations: Mutex<()>,
    segments: Arc<SegmentWriter>,
    index: Arc<SqliteIndex>,
    removal: RemovalWorker,
    budget: DiskBudgetBytes,
    open_overhead_limit: u64,
    budget_state: StdMutex<RecordingBudgetState>,
    availability: watch::Sender<u64>,
    deleted_sessions: StdMutex<BTreeSet<SessionId>>,
}

impl RecordingStore {
    pub fn new(
        segments: Arc<SegmentWriter>,
        index: Arc<SqliteIndex>,
    ) -> krometrail_core::Result<Self> {
        Self::with_budget(segments, index, DiskBudgetBytes::default())
    }

    pub fn with_budget(
        segments: Arc<SegmentWriter>,
        index: Arc<SqliteIndex>,
        budget: DiskBudgetBytes,
    ) -> krometrail_core::Result<Self> {
        let data_directory = index
            .database_path()
            .parent()
            .ok_or_else(|| persistence_error("recording index has no data directory"))?
            .to_path_buf();
        let removal =
            RemovalWorker::open(data_directory, index.segments_directory().to_path_buf())?;
        let (_receiver, availability) = {
            let (sender, receiver) = watch::channel(0_u64);
            (receiver, sender)
        };
        let store = Self {
            mutations: Mutex::new(()),
            open_overhead_limit: segments
                .rotation_max_size()
                .saturating_add(RECORD_ENVELOPE_ALLOWANCE),
            segments,
            index,
            removal,
            budget,
            budget_state: StdMutex::new(RecordingBudgetState::Available),
            availability,
            deleted_sessions: StdMutex::new(BTreeSet::new()),
        };
        store.resume_deletions()?;
        Ok(store)
    }

    pub fn index(&self) -> &Arc<SqliteIndex> {
        &self.index
    }

    fn resume_deletions(&self) -> krometrail_core::Result<()> {
        for batch in self.index.deletion_batches()? {
            if batch.state == DeletionState::Prepared {
                self.removal.stage_blocking(batch.clone())?;
                self.index.remove_deletion_metadata(&batch)?;
            }
            self.removal.finalize_blocking(batch.clone())?;
            self.index.finalize_deletion(&batch)?;
        }
        Ok(())
    }

    fn is_deleted(&self, session_id: SessionId) -> bool {
        self.deleted_sessions
            .lock()
            .expect("deleted session lock poisoned")
            .contains(&session_id)
    }

    fn reject_deleted(&self, session_id: SessionId) -> krometrail_core::Result<()> {
        if self.is_deleted(session_id) {
            return Err(KrometrailError::new(
                ErrorCode::NotFound,
                NonEmptyText::new("recording session has been deleted")
                    .expect("static deletion error is non-empty"),
            )
            .with_context(ErrorContext {
                session_id: Some(session_id),
                ..ErrorContext::default()
            }));
        }
        Ok(())
    }

    async fn register_segments(
        &self,
        registrations: &[SegmentRegistration],
    ) -> krometrail_core::Result<()> {
        if registrations.is_empty() {
            return Ok(());
        }
        let mut connection = self.index.connection()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin segment registration"))?;
        for registration in registrations {
            register_segment_tx(&transaction, registration)?;
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit segment registration"))
    }

    async fn flush_all(&self) -> krometrail_core::Result<()> {
        let registrations = self.segments.flush_all_indexable().await?;
        self.register_segments(&registrations).await
    }

    async fn flush_session(&self, session_id: SessionId) -> krometrail_core::Result<()> {
        let registrations = self.segments.flush_indexable(session_id).await?;
        self.register_segments(&registrations).await
    }

    fn set_budget_state(&self, next: RecordingBudgetState) {
        let mut state = self
            .budget_state
            .lock()
            .expect("budget state lock poisoned");
        if *state != next {
            *state = next;
            if next == RecordingBudgetState::Available {
                self.availability.send_modify(|generation| {
                    *generation = generation.saturating_add(1);
                });
            }
        }
    }

    fn current_budget_state(&self) -> RecordingBudgetState {
        *self
            .budget_state
            .lock()
            .expect("budget state lock poisoned")
    }

    fn refresh_usage(&self) -> krometrail_core::Result<crate::index::retention::UsageSnapshot> {
        self.index.refresh_index_usage()?;
        self.index.usage_snapshot()
    }

    fn status_from_snapshot(
        &self,
        snapshot: crate::index::retention::UsageSnapshot,
        state: RecordingBudgetState,
    ) -> krometrail_core::Result<RetentionStatus> {
        let usage = StorageUsage::new(
            snapshot.usage.segment_bytes,
            snapshot.usage.index_bytes,
            snapshot.usage.browser_event_bytes,
            snapshot.usage.artifact_bytes,
            snapshot.usage.pending_deletion_bytes,
            snapshot.usage.open_segment_bytes,
            SQLITE_ACCOUNTING_SLACK,
        )?;
        RetentionStatus::new(
            self.budget,
            usage,
            snapshot.pinned_usage_bytes,
            snapshot.oldest_retained,
            snapshot.newest_retained,
            state,
            state == RecordingBudgetState::PausedBudget,
            state == RecordingBudgetState::PausedBudget,
            snapshot.open_segment_count,
            snapshot.usage.open_segment_bytes,
            self.open_overhead_limit,
        )
    }

    fn current_status(&self) -> krometrail_core::Result<RetentionStatus> {
        self.status_from_snapshot(self.refresh_usage()?, self.current_budget_state())
    }

    async fn ensure_append_capacity(&self, frame: &EncodedFrame) -> krometrail_core::Result<()> {
        let required = frame
            .byte_len()
            .get()
            .checked_add(RECORD_ENVELOPE_ALLOWANCE)
            .ok_or_else(|| persistence_error("frame storage estimate overflow"))?;
        let snapshot = self.refresh_usage()?;
        if snapshot
            .usage
            .total_bytes()?
            .checked_add(required)
            .is_some_and(|needed| needed <= self.budget.get())
        {
            return Ok(());
        }
        self.flush_all().await?;
        self.cleanup_to(self.budget.get().saturating_sub(required))
            .await?;
        let snapshot = self.refresh_usage()?;
        if snapshot
            .usage
            .total_bytes()?
            .checked_add(required)
            .is_some_and(|needed| needed <= self.budget.get())
        {
            self.set_budget_state(RecordingBudgetState::Available);
            Ok(())
        } else {
            self.set_budget_state(RecordingBudgetState::PausedBudget);
            Err(budget_error(
                frame.metadata().session_id(),
                frame.metadata().target_id(),
            ))
        }
    }

    async fn enforce_locked(&self) -> krometrail_core::Result<RetentionStatus> {
        let mut snapshot = self.refresh_usage()?;
        let total = snapshot.usage.total_bytes()?;
        if total <= self.budget.get()
            || (snapshot.open_segment_count <= 1
                && total.saturating_sub(self.budget.get()) <= self.open_overhead_limit)
        {
            self.set_budget_state(RecordingBudgetState::Available);
            return self.status_from_snapshot(snapshot, RecordingBudgetState::Available);
        }
        self.flush_all().await?;
        self.cleanup_to(self.budget.get()).await?;
        snapshot = self.refresh_usage()?;
        let state = if snapshot.usage.total_bytes()? <= self.budget.get() {
            RecordingBudgetState::Available
        } else {
            RecordingBudgetState::PausedBudget
        };
        self.set_budget_state(state);
        self.status_from_snapshot(snapshot, state)
    }

    async fn cleanup_to(&self, target_bytes: u64) -> krometrail_core::Result<()> {
        loop {
            if self.refresh_usage()?.usage.total_bytes()? <= target_bytes {
                return Ok(());
            }
            if let Some(artifact) = self.index.oldest_artifact()? {
                self.remove_objects(
                    DeletionKind::Eviction,
                    None,
                    vec![artifact_object(artifact)],
                )
                .await?;
                continue;
            }
            let Some(segment) = self.index.oldest_unpinned_segment()? else {
                return Ok(());
            };
            let mut objects: Vec<_> = self
                .index
                .artifacts_for_segment(segment.segment_id)?
                .into_iter()
                .map(artifact_object)
                .collect();
            objects.push(segment_object(segment));
            self.remove_objects(DeletionKind::Eviction, None, objects)
                .await?;
        }
    }

    async fn remove_objects(
        &self,
        kind: DeletionKind,
        session_id: Option<SessionId>,
        objects: Vec<DeletionObject>,
    ) -> krometrail_core::Result<(u64, u64, u64, u64)> {
        let removed_bytes = objects.iter().try_fold(0_u64, |total, object| {
            total
                .checked_add(object.byte_len)
                .ok_or_else(|| persistence_error("deleted byte count overflow"))
        })?;
        let batch = self.index.prepare_deletion(kind, session_id, objects)?;
        self.removal.stage(batch.clone()).await?;
        let (segments, frames, artifacts) = self.index.remove_deletion_metadata(&batch)?;
        let mut committed = batch.clone();
        committed.state = DeletionState::MetadataRemoved;
        self.removal.finalize(committed).await?;
        self.index.finalize_deletion(&batch)?;
        Ok((segments, frames, artifacts, removed_bytes))
    }
}

impl RecordingSink for RecordingStore {
    fn append_frame(
        &self,
        frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(frame.metadata().session_id())?;
            self.ensure_append_capacity(&frame).await?;
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
            self.reject_deleted(gap.session_id())?;
            CaptureGapStore::append_gap(self.index.as_ref(), gap).await
        })
    }

    fn flush(&self, session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(session_id)?;
            self.flush_session(session_id).await?;
            self.enforce_locked().await.map(|_| ())
        })
    }
}

impl RetentionStore for RecordingStore {
    fn pin_range(
        &self,
        request: RetentionRange,
    ) -> PortFuture<'_, krometrail_core::Result<PinChange>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.session_id)?;
            self.flush_session(request.session_id).await?;
            self.index.pin_range(request)
        })
    }

    fn unpin_range(
        &self,
        request: RetentionRange,
    ) -> PortFuture<'_, krometrail_core::Result<PinChange>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            let change = self.index.unpin_range(request)?;
            self.enforce_locked().await?;
            Ok(change)
        })
    }

    fn enforce_budget(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.enforce_locked().await
        })
    }

    fn status(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.current_status()
        })
    }

    fn delete_session(
        &self,
        session_id: SessionId,
    ) -> PortFuture<'_, krometrail_core::Result<SessionDeletion>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            if self.is_deleted(session_id) {
                return Ok(SessionDeletion {
                    session_id,
                    removed_segments: 0,
                    removed_frames: 0,
                    removed_artifacts: 0,
                    removed_bytes: 0,
                });
            }
            self.flush_session(session_id).await?;
            let session_usage = self.index.session_usage_bytes(session_id)?;
            let segments = self.index.session_segments(session_id)?;
            let mut artifacts = self.index.session_artifacts(session_id)?;
            let mut seen: HashSet<_> = artifacts.iter().map(|item| item.artifact_id).collect();
            for segment in &segments {
                for artifact in self.index.artifacts_for_segment(segment.segment_id)? {
                    if seen.insert(artifact.artifact_id) {
                        artifacts.push(artifact);
                    }
                }
            }
            let mut objects: Vec<_> = artifacts.into_iter().map(artifact_object).collect();
            objects.extend(segments.into_iter().map(segment_object));
            self.deleted_sessions
                .lock()
                .expect("deleted session lock poisoned")
                .insert(session_id);
            let (removed_segments, removed_frames, removed_artifacts, object_bytes) = self
                .remove_objects(DeletionKind::Session, Some(session_id), objects)
                .await?;
            self.enforce_locked().await?;
            Ok(SessionDeletion {
                session_id,
                removed_segments,
                removed_frames,
                removed_artifacts,
                removed_bytes: session_usage.max(object_bytes),
            })
        })
    }

    fn wait_until_recording_allowed(&self) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let mut receiver = self.availability.subscribe();
            loop {
                if self.current_budget_state() == RecordingBudgetState::Available {
                    return Ok(());
                }
                receiver
                    .changed()
                    .await
                    .map_err(|_| persistence_error("budget availability notifier stopped"))?;
            }
        })
    }
}

fn artifact_object(candidate: ArtifactCandidate) -> DeletionObject {
    DeletionObject {
        kind: DeletionObjectKind::Artifact(candidate.artifact_id),
        relative_path: candidate.relative_path,
        byte_len: candidate.byte_len,
        session_id: candidate.session_id,
    }
}

fn segment_object(candidate: SegmentCandidate) -> DeletionObject {
    DeletionObject {
        kind: DeletionObjectKind::Segment(candidate.segment_id),
        relative_path: candidate.relative_path,
        byte_len: candidate.file_bytes,
        session_id: candidate.session_id,
    }
}

fn budget_error(session_id: SessionId, target_id: krometrail_core::TargetId) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::BudgetExhausted,
        NonEmptyText::new("disk budget paused capture").expect("static budget error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        ..ErrorContext::default()
    })
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("unpin or delete retained evidence, or increase the disk budget")
            .expect("static budget recovery is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use krometrail_core::{
        ArtifactId, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId,
        FrameSource, ImageFormat, ObservedTime, PixelDimensions, RecordingSink, SessionTime,
        TargetId,
    };
    use rusqlite::params;
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::index::{
        codec,
        maintenance::{UsageClass, UsageEntry},
    };
    use crate::{IndexStoreConfig, RotationConfig, SegmentStoreConfig};

    use super::*;

    fn frame(session: u128, target: u128, id: u128, ordinal: u64) -> EncodedFrame {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(id)),
                SessionId::from_uuid(Uuid::from_u128(session)),
                TargetId::from_uuid(Uuid::from_u128(target)),
                CaptureOrdinal::new(ordinal).unwrap(),
                None,
                ObservedTime::from_nanos(ordinal),
                SessionTime::from_nanos(ordinal),
                ImageFormat::Jpeg,
                PixelDimensions::new(1, 1).unwrap(),
                PixelDimensions::new(1, 1).unwrap(),
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![],
            )
            .unwrap(),
            vec![1; 32],
        )
        .unwrap()
    }

    fn fixture(directory: &TempDir) -> (Arc<SqliteIndex>, Arc<SegmentWriter>, RecordingStore) {
        let segments = directory.path().join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: directory.path().join("index.sqlite3"),
                segments_directory: segments.clone(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        );
        let writer = Arc::new(
            SegmentWriter::open(SegmentStoreConfig {
                directory: segments,
                rotation: RotationConfig {
                    max_duration: Duration::from_secs(1),
                    max_size: 1,
                },
            })
            .unwrap(),
        );
        let store = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
        (index, writer, store)
    }

    #[tokio::test]
    async fn mixed_source_artifact_is_staged_before_any_source_segment_disappears() {
        let directory = TempDir::new().unwrap();
        let (index, _writer, store) = fixture(&directory);
        let first = frame(1, 2, 3, 1);
        let second = frame(1, 2, 4, 2);
        store.append_frame(first.clone()).await.unwrap();
        store.append_frame(second.clone()).await.unwrap();
        store.flush(first.metadata().session_id()).await.unwrap();

        let artifact_id = ArtifactId::from_uuid(Uuid::from_u128(10));
        let surviving_artifact_id = ArtifactId::from_uuid(Uuid::from_u128(11));
        let artifact_path = directory.path().join("artifacts").join("mixed.png");
        let surviving_artifact_path = directory.path().join("artifacts").join("surviving.png");
        std::fs::write(&artifact_path, b"artifact").unwrap();
        std::fs::write(&surviving_artifact_path, b"survives").unwrap();
        {
            let connection = index.connection().unwrap();
            connection.execute(
                "INSERT INTO artifacts(artifact_id, session_id, target_id, kind, start_time_be, \
                 end_time_be, manifest_json, relative_path, byte_len_be) VALUES \
                 (?1, ?2, ?3, 'storyboard', ?4, ?5, '{}', 'mixed.png', ?6)",
                params![
                    codec::id(artifact_id.as_uuid()).to_vec(),
                    codec::id(first.metadata().session_id().as_uuid()).to_vec(),
                    codec::id(first.metadata().target_id().as_uuid()).to_vec(),
                    codec::u64_blob(1).to_vec(), codec::u64_blob(2).to_vec(),
                    codec::u64_blob(8).to_vec(),
                ],
            ).unwrap();
            for (position, id) in [first.metadata().id(), second.metadata().id()]
                .into_iter()
                .enumerate()
            {
                connection.execute(
                    "INSERT INTO artifact_frames(artifact_id, source_position, frame_id) VALUES (?1, ?2, ?3)",
                    params![codec::id(artifact_id.as_uuid()).to_vec(), position as i64, codec::id(id.as_uuid()).to_vec()],
                ).unwrap();
            }
            connection.execute(
                "INSERT INTO artifacts(artifact_id, session_id, target_id, kind, start_time_be, \
                 end_time_be, manifest_json, relative_path, byte_len_be) VALUES \
                 (?1, ?2, ?3, 'storyboard', ?4, ?5, '{}', 'surviving.png', ?6)",
                params![
                    codec::id(surviving_artifact_id.as_uuid()).to_vec(),
                    codec::id(first.metadata().session_id().as_uuid()).to_vec(),
                    codec::id(first.metadata().target_id().as_uuid()).to_vec(),
                    codec::u64_blob(1).to_vec(), codec::u64_blob(2).to_vec(),
                    codec::u64_blob(8).to_vec(),
                ],
            ).unwrap();
            connection.execute(
                "INSERT INTO artifact_frames(artifact_id, source_position, frame_id) VALUES (?1, 0, ?2)",
                params![
                    codec::id(surviving_artifact_id.as_uuid()).to_vec(),
                    codec::id(second.metadata().id().as_uuid()).to_vec(),
                ],
            ).unwrap();
        }
        for artifact_id in [artifact_id, surviving_artifact_id] {
            index
                .update_usage(UsageEntry {
                    class: UsageClass::Artifact,
                    object_key: codec::id(artifact_id.as_uuid()).to_vec().into_boxed_slice(),
                    session_id: Some(first.metadata().session_id()),
                    byte_len: 8,
                })
                .unwrap();
        }

        let candidate = index.oldest_unpinned_segment().unwrap().unwrap();
        let mut objects: Vec<_> = index
            .artifacts_for_segment(candidate.segment_id)
            .unwrap()
            .into_iter()
            .map(artifact_object)
            .collect();
        objects.push(segment_object(candidate));
        store
            .remove_objects(DeletionKind::Eviction, None, objects)
            .await
            .unwrap();

        assert!(!artifact_path.exists());
        assert!(surviving_artifact_path.exists());
        let artifact_count: u64 = index
            .connection()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM artifacts WHERE artifact_id=?1",
                params![codec::id(artifact_id.as_uuid()).to_vec()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifact_count, 0);
        let surviving_count: u64 = index
            .connection()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM artifacts WHERE artifact_id=?1",
                params![codec::id(surviving_artifact_id.as_uuid()).to_vec()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(surviving_count, 1);
        assert!(
            index
                .frames_by_id(vec![first.metadata().id()])
                .await
                .is_err()
        );
        assert_eq!(
            index
                .frames_by_id(vec![second.metadata().id()])
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn constructor_replays_prepared_and_metadata_removed_deletion_phases() {
        for metadata_removed in [false, true] {
            let directory = TempDir::new().unwrap();
            let (index, writer, store) = fixture(&directory);
            let item = frame(20, 21, 22, 1);
            store.append_frame(item.clone()).await.unwrap();
            store.flush(item.metadata().session_id()).await.unwrap();
            let candidate = index.oldest_unpinned_segment().unwrap().unwrap();
            let candidate_bytes = candidate.file_bytes;
            let batch = index
                .prepare_deletion(
                    DeletionKind::Eviction,
                    None,
                    vec![segment_object(candidate)],
                )
                .unwrap();
            store.removal.stage_blocking(batch.clone()).unwrap();
            assert_eq!(
                store.status().await.unwrap().usage.pending_deletion_bytes,
                candidate_bytes
            );
            if metadata_removed {
                index.remove_deletion_metadata(&batch).unwrap();
            }
            drop(store);

            let reopened = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
            assert!(index.deletion_batches().unwrap().is_empty());
            assert!(
                index
                    .frames_by_id(vec![item.metadata().id()])
                    .await
                    .is_err()
            );
            let status = reopened.status().await.unwrap();
            assert_eq!(status.usage.pending_deletion_bytes, 0);
            assert!(status.usage.segment_bytes < candidate_bytes);
        }
    }
}
