use std::{
    collections::HashSet,
    sync::{Arc, Mutex as StdMutex},
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, CaptureGap, CaptureGapStore, DiskBudgetBytes,
    EncodedFrame, ErrorCode, ErrorContext, FrameAddress, FrameSource, InteractionAnchor,
    InteractionEvidenceSink, InteractionRecord, KrometrailError, NavigationId, NonEmptyText,
    ObservedTime, PinChange, PortFuture, RecordingBudgetState, RecordingSink, ResolvedRange,
    RetentionRange, RetentionStatus, RetentionStore, RetryAdvice, SessionDeletion, SessionId,
    SessionRange, StorageUsage, StoredArtifact, TargetId, TemporalQuery, TemporalQueryRequest,
    TemporalQueryService, TemporalRangeResolver, TimelineObservation, TimelineStore,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, watch};

use crate::{
    SegmentRegistration, SegmentWriter, SqliteIndex,
    artifacts::{
        CacheLocks, PublicationRegistry, files::ArtifactFiles, recovery as artifact_recovery,
        source_fingerprints, validate_stored_artifact,
    },
    index::{
        artifacts::{ArtifactRow, ArtifactState, StageArtifact},
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
    artifact_files: ArtifactFiles,
    artifact_publications: PublicationRegistry,
    artifact_cache_locks: CacheLocks,
    budget: DiskBudgetBytes,
    open_overhead_limit: u64,
    budget_state: StdMutex<RecordingBudgetState>,
    availability: watch::Sender<u64>,
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
        let removal = RemovalWorker::open(
            data_directory.clone(),
            index.segments_directory().to_path_buf(),
        )?;
        let artifact_files = ArtifactFiles::open(data_directory.join("artifacts"))?;
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
            artifact_files,
            artifact_publications: PublicationRegistry::new(),
            artifact_cache_locks: CacheLocks::new(),
            budget,
            budget_state: StdMutex::new(RecordingBudgetState::Available),
            availability,
        };
        store.resume_deletions()?;
        store.recover_artifacts()?;
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
        self.artifact_publications.is_deleted(session_id)
    }

    fn recover_artifacts(&self) -> krometrail_core::Result<()> {
        let mut plan = artifact_recovery::plan(self.index.as_ref(), &self.artifact_files)?;
        for candidate in plan.invalid.drain(..) {
            self.remove_objects_blocking(
                DeletionKind::Eviction,
                None,
                vec![artifact_object(candidate)],
            )?;
        }
        plan.report.usage_rows_reconciled =
            artifact_recovery::reconcile_usage(self.index.as_ref())?;
        tracing::info!(
            staging_finalized = plan.report.staging_finalized,
            staging_removed = plan.report.staging_removed,
            ready_invalidated = plan.report.ready_invalidated,
            orphan_files_removed = plan.report.orphan_files_removed,
            usage_rows_reconciled = plan.report.usage_rows_reconciled,
            "artifact store recovery complete"
        );
        Ok(())
    }

    fn remove_objects_blocking(
        &self,
        kind: DeletionKind,
        session_id: Option<SessionId>,
        objects: Vec<DeletionObject>,
    ) -> krometrail_core::Result<()> {
        let batch = self.index.prepare_deletion(kind, session_id, objects)?;
        self.removal.stage_blocking(batch.clone())?;
        self.index.remove_deletion_metadata(&batch)?;
        let mut committed = batch.clone();
        committed.state = DeletionState::MetadataRemoved;
        self.removal.finalize_blocking(committed)?;
        self.index.finalize_deletion(&batch)
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

    async fn validate_source_payloads(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        expected: &[ArtifactSourceFingerprint],
    ) -> krometrail_core::Result<()> {
        let frames = self
            .index
            .frames_by_id(expected.iter().map(|source| source.frame_id).collect())
            .await?;
        if frames.len() != expected.len() {
            return Err(source_lost_error(session_id, target_id));
        }
        for (frame, expected) in frames.iter().zip(expected) {
            if frame.metadata().id() != expected.frame_id
                || frame.metadata().session_id() != session_id
                || frame.metadata().target_id() != target_id
                || <[u8; 32]>::from(Sha256::digest(frame.bytes())) != expected.encoded_sha256
            {
                return Err(source_lost_error(session_id, target_id));
            }
        }
        Ok(())
    }

    async fn read_artifact_row(
        &self,
        row: &ArtifactRow,
        expected: Option<&[ArtifactSourceFingerprint]>,
    ) -> krometrail_core::Result<StoredArtifact> {
        if row.state != ArtifactState::Ready {
            return Err(persistence_error("artifact is not ready"));
        }
        let sources = self.index.artifact_sources(row.artifact_id)?;
        let retained = source_fingerprints(&sources);
        self.validate_source_payloads(row.session_id, row.target_id, &retained)
            .await?;
        let bytes = self.artifact_files.read(row.relative_path.clone()).await?;
        validate_stored_artifact(row, &sources, bytes, expected)
    }

    async fn invalidate_artifact_row(&self, row: ArtifactRow) -> krometrail_core::Result<()> {
        self.remove_objects(
            DeletionKind::Eviction,
            None,
            vec![artifact_object(ArtifactCandidate {
                artifact_id: row.artifact_id,
                session_id: row.session_id,
                relative_path: row.relative_path,
                byte_len: row.byte_len,
            })],
        )
        .await
        .map(|_| ())
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

impl ArtifactStore for RecordingStore {
    fn lookup_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactLookup>> {
        Box::pin(async move {
            let row = {
                let _mutation = self.mutations.lock().await;
                self.index.artifact_by_cache(key, true)?
            };
            let Some(row) = row else {
                return Ok(ArtifactLookup::Miss);
            };
            let validated = self.read_artifact_row(&row, Some(&expected_sources)).await;
            let _mutation = self.mutations.lock().await;
            let unchanged = self
                .index
                .artifact_by_cache(key, true)?
                .is_some_and(|current| current == row);
            if !unchanged {
                return Ok(ArtifactLookup::Miss);
            }
            match validated {
                Ok(artifact) => Ok(ArtifactLookup::Hit(Box::new(artifact))),
                Err(_) => {
                    self.invalidate_artifact_row(row).await?;
                    Ok(ArtifactLookup::Invalidated)
                }
            }
        })
    }

    fn publish_artifact(
        &self,
        publication: ArtifactPublication,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactPublish>> {
        Box::pin(async move {
            let publication_guard = self.artifact_publications.begin(publication.session_id)?;
            let cache_lock = self
                .artifact_cache_locks
                .for_key(publication.cache.cache_key);
            let _cache = cache_lock.lock().await;

            match self
                .lookup_artifact(publication.cache.cache_key, publication.sources.clone())
                .await?
            {
                ArtifactLookup::Hit(artifact) => return Ok(ArtifactPublish::Existing(*artifact)),
                ArtifactLookup::Miss | ArtifactLookup::Invalidated => {}
            }
            self.validate_source_payloads(
                publication.session_id,
                publication.target_id,
                &publication.sources,
            )
            .await?;

            let staged = {
                let _mutation = self.mutations.lock().await;
                self.reject_deleted(publication.session_id)?;
                if publication_guard.is_cancelled() {
                    return Err(cancelled_publication_error());
                }
                self.index.stage_artifact(&publication)?
            };
            let row = match staged {
                StageArtifact::Staged(row) => row,
                StageArtifact::Existing(existing) if existing.state == ArtifactState::Ready => {
                    let stored = self
                        .read_artifact_row(&existing, Some(&publication.sources))
                        .await?;
                    return Ok(ArtifactPublish::Existing(stored));
                }
                StageArtifact::Existing(existing) => {
                    let _mutation = self.mutations.lock().await;
                    self.invalidate_artifact_row(existing).await?;
                    return Err(persistence_error(
                        "stale artifact staging state was invalidated; retry publication",
                    ));
                }
            };

            if let Err(error) = self
                .artifact_files
                .publish(
                    row.artifact_id,
                    Arc::clone(&publication.encoded_bytes),
                    publication_guard.cancellation(),
                    publication.cancellation().cloned(),
                )
                .await
            {
                let _mutation = self.mutations.lock().await;
                self.invalidate_artifact_row(row).await?;
                return Err(error);
            }

            let finalized = {
                let _mutation = self.mutations.lock().await;
                if publication_guard.is_cancelled()
                    || publication
                        .cancellation()
                        .is_some_and(|signal| signal.is_cancelled())
                    || self.is_deleted(publication.session_id)
                {
                    self.invalidate_artifact_row(row.clone()).await?;
                    return Err(cancelled_publication_error());
                }
                self.index.finalize_artifact(
                    row.artifact_id,
                    publication.cache.cache_key,
                    publication.session_id,
                    publication.target_id,
                    &publication.sources,
                )?
            };
            if !finalized {
                let _mutation = self.mutations.lock().await;
                self.invalidate_artifact_row(row).await?;
                return Err(persistence_error(
                    "artifact publication did not reach ready state",
                ));
            }
            let ready = self
                .index
                .artifact_row(*publication.manifest.artifact_id())?
                .ok_or_else(|| persistence_error("ready artifact metadata disappeared"))?;
            let stored = self
                .read_artifact_row(&ready, Some(&publication.sources))
                .await?;
            Ok(ArtifactPublish::Published(stored))
        })
    }

    fn artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredArtifact>>> {
        Box::pin(async move {
            let row = {
                let _mutation = self.mutations.lock().await;
                self.index
                    .artifact_row(artifact_id)?
                    .filter(|row| row.state == ArtifactState::Ready)
            };
            let Some(row) = row else {
                return Ok(None);
            };
            match self.read_artifact_row(&row, None).await {
                Ok(artifact) => Ok(Some(artifact)),
                Err(_) => {
                    let _mutation = self.mutations.lock().await;
                    if self
                        .index
                        .artifact_row(artifact_id)?
                        .is_some_and(|current| current == row)
                    {
                        self.invalidate_artifact_row(row).await?;
                    }
                    Ok(None)
                }
            }
        })
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

impl InteractionEvidenceSink for RecordingStore {
    fn append_operation_evidence(
        &self,
        anchor: InteractionAnchor,
        record: Option<InteractionRecord>,
        persisted_at: ObservedTime,
        navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(anchor.session_id)?;
            self.index.append_operation_evidence(
                &anchor,
                record.as_ref(),
                persisted_at,
                navigation_id,
            )
        })
    }
}

impl TimelineStore for RecordingStore {
    fn append(
        &self,
        observation: TimelineObservation,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(observation.session_id())?;
            TimelineStore::append(self.index.as_ref(), observation).await
        })
    }

    fn range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<TimelineObservation>>> {
        TimelineStore::range(self.index.as_ref(), session_id, target_id, range)
    }
}

impl TemporalQuery for RecordingStore {
    fn resolve_range(
        &self,
        request: TemporalQueryRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ResolvedRange>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            TemporalQueryService::new(TemporalRangeResolver::new(
                Arc::clone(&self.index),
                Arc::clone(&self.index),
                Arc::clone(&self.index),
                Arc::clone(&self.index),
                Arc::clone(&self.index),
            ))
            .resolve_range(request)
            .await
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
            // Fence publication before waiting: no new publisher can enter, active file work
            // observes cancellation, and the mutation gate remains available while it drains.
            self.artifact_publications.mark_deleted(session_id);
            self.artifact_publications.drain(session_id).await;
            let _mutation = self.mutations.lock().await;
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

fn source_lost_error(session_id: SessionId, target_id: TargetId) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("artifact source evidence is no longer retained")
            .expect("static source error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        ..Default::default()
    })
}

fn cancelled_publication_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("artifact publication was cancelled")
            .expect("static publication cancellation is non-empty"),
    )
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
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId, FrameSource,
        ImageFormat, ObservedTime, PixelDimensions, RecordingSink, SessionTime, TargetId,
    };
    use tempfile::TempDir;
    use uuid::Uuid;

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
    async fn eviction_ranges_coalesce_and_session_deletion_removes_them() {
        let directory = TempDir::new().unwrap();
        let (index, _writer, store) = fixture(&directory);
        let first = frame(30, 31, 32, 1);
        let second = frame(30, 31, 33, 2);
        store.append_frame(first.clone()).await.unwrap();
        store.append_frame(second.clone()).await.unwrap();
        store.flush(first.metadata().session_id()).await.unwrap();

        for _ in 0..2 {
            let candidate = index.oldest_unpinned_segment().unwrap().unwrap();
            store
                .remove_objects(
                    DeletionKind::Eviction,
                    None,
                    vec![segment_object(candidate)],
                )
                .await
                .unwrap();
        }
        let availability = index
            .frame_availability(first.metadata().session_id(), first.metadata().target_id())
            .await
            .unwrap();
        assert_eq!(availability.retained_bounds, None);
        assert_eq!(
            availability.evicted_ranges,
            vec![
                krometrail_core::SessionRange::new(
                    SessionTime::from_nanos(1),
                    SessionTime::from_nanos(2),
                )
                .unwrap()
            ]
        );

        store
            .delete_session(first.metadata().session_id())
            .await
            .unwrap();
        let deleted = index
            .frame_availability(first.metadata().session_id(), first.metadata().target_id())
            .await
            .unwrap();
        assert_eq!(deleted.retained_bounds, None);
        assert!(deleted.evicted_ranges.is_empty());
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
