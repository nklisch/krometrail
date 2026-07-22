use std::{
    collections::HashSet,
    sync::{Arc, Mutex as StdMutex},
};

#[cfg(feature = "qualification-support")]
use std::fs;

use krometrail_core::{
    ArtifactCacheKey, ArtifactEvidenceHandle, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactRead, ArtifactReadLookup, ArtifactSourceFingerprint, ArtifactStore, BrowserEventBatch,
    BrowserEventCursor, BrowserEventSelector, BrowserEventSink, BrowserEventSource,
    BrowserEventUnavailableRange, CaptureGap, CaptureGapStore, CaptureStatusSamples,
    DiskBudgetBytes, EncodedFrame, ErrorCode, ErrorContext, EventCandidateLimit, EventPageLimit,
    EvidenceScope, FrameAddress, FrameId, FrameSource, InteractionAnchor, InteractionAnchorSource,
    InteractionEvidenceSink, InteractionId, InteractionRecord, InteractionRecordSource,
    KrometrailError, MonotonicClock, NavigationId, NonEmptyText, ObservedTime,
    PersistenceOperation, PinChange, PinProtectionScope, PinState, PortFuture,
    ProgressivePinChange, RecordingBudgetState, RecordingSink, ResolvedRange, RetentionLifecycle,
    RetentionPinRequest, RetentionRange, RetentionStatus, RetentionStore, RetrieveArtifactRequest,
    RetrieveSourceFrameRequest, RetryAdvice, SessionDeletion, SessionId, SessionRange, SessionTime,
    Sha256Digest, SourceFrameBatch, SourceFrameHandle, SourceFrameList, SourceFrameRead,
    SourceFrameSelection, SourceFramesRequest, StorageUsage, StoredArtifact, StoredVideoArtifact,
    TargetId, TemporalContext, TemporalContextQuery, TemporalContextRequest,
    TemporalContextService, TemporalQuery, TemporalQueryRequest, TemporalQueryService,
    TemporalRangeResolver, TimelineObservation, TimelineStore, VideoArtifactEvidenceHandle,
    VideoArtifactLookup, VideoArtifactPublication, VideoArtifactPublish, VideoArtifactRead,
    VideoArtifactReadLookup,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, watch};

use crate::{
    SegmentRegistration, SegmentWriter, SqliteIndex,
    artifacts::{
        CacheLocks, PublicationRegistry, RetainedStoredArtifact, files::ArtifactFiles,
        recovery as artifact_recovery, validate_stored_artifact,
    },
    index::{
        artifacts::{
            ArtifactRow, ArtifactSourceRow, ArtifactState, RetainedArtifactKind, StageArtifact,
        },
        deletion::{DeletionKind, DeletionObject, DeletionObjectKind, DeletionState},
        frames::{FrameReadSnapshot, index_frame_tx},
        retention::{ArtifactCandidate, SegmentCandidate, SegmentReclaimFilter},
        segments::register_segment_tx,
    },
    persistence_error,
    retention::removal::RemovalWorker,
};

const RECORD_ENVELOPE_ALLOWANCE: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EvidenceReadKind {
    Source,
    Artifact,
}

#[cfg(test)]
struct EvidenceReadPause {
    kind: EvidenceReadKind,
    reached: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

/// What one reclaim walk actually removed.
///
/// Reclaim is otherwise invisible to the agent: usage simply moves. Reporting
/// the counts makes trimming and age-out observable in the runtime log instead
/// of leaving evidence to disappear silently.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReclaimOutcome {
    segments: u64,
    artifacts: u64,
    browser_events: u64,
    bytes: u64,
    artifact_grace_overridden: bool,
}

impl ReclaimOutcome {
    const fn reclaimed_anything(self) -> bool {
        self.segments != 0 || self.artifacts != 0 || self.browser_events != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ArtifactReadSnapshot {
    row: ArtifactRow,
    sources: Vec<ArtifactSourceRow>,
    frames: Vec<FrameReadSnapshot>,
}

/// Coordinates physical frame writes, searchable metadata, and destructive
/// retention mutations behind one ordering gate.
pub struct RecordingStore {
    mutations: Mutex<()>,
    /// Injected process clock; the range resolver uses it to refine live-session
    /// partial tails as "not yet elapsed". No default clock exists in the store.
    clock: Arc<dyn MonotonicClock>,
    segments: Arc<SegmentWriter>,
    index: Arc<SqliteIndex>,
    removal: RemovalWorker,
    artifact_files: ArtifactFiles,
    artifact_publications: PublicationRegistry,
    artifact_cache_locks: CacheLocks,
    retention: RetentionLifecycle,
    open_overhead_limit: u64,
    budget_state: StdMutex<RecordingBudgetState>,
    /// Set when a trim walk found nothing to reclaim; cleared by any later
    /// reclamation so trimming resumes once the store can make progress again.
    trim_exhausted: StdMutex<bool>,
    /// Live-instance count that divides one total budget across concurrent
    /// instances. Absent when this store is not part of a multi-instance data
    /// directory, in which case the configured budget is enforced directly.
    census: Option<crate::InstanceCensus>,
    availability: watch::Sender<u64>,
    #[cfg(test)]
    evidence_read_pause: StdMutex<Option<Arc<EvidenceReadPause>>>,
}

impl RecordingStore {
    pub fn new(
        segments: Arc<SegmentWriter>,
        index: Arc<SqliteIndex>,
        clock: Arc<dyn MonotonicClock>,
    ) -> krometrail_core::Result<Self> {
        Self::with_retention(segments, index, RetentionLifecycle::default(), None, clock)
    }

    pub fn with_budget(
        segments: Arc<SegmentWriter>,
        index: Arc<SqliteIndex>,
        budget: DiskBudgetBytes,
        clock: Arc<dyn MonotonicClock>,
    ) -> krometrail_core::Result<Self> {
        Self::with_retention(
            segments,
            index,
            RetentionLifecycle::with_budget(budget),
            None,
            clock,
        )
    }

    /// Opens the store under a complete retention lifecycle.
    ///
    /// Callers must have completed `recover()` against this index first; the
    /// constructor verifies that invariant rather than trusting it.
    ///
    /// `census` divides one total budget across concurrent instances. Without it
    /// the configured budget is enforced by this instance alone.
    pub fn with_retention(
        segments: Arc<SegmentWriter>,
        index: Arc<SqliteIndex>,
        retention: RetentionLifecycle,
        census: Option<crate::InstanceCensus>,
        clock: Arc<dyn MonotonicClock>,
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
            clock,
            open_overhead_limit: segments
                .rotation_max_size()
                .saturating_add(RECORD_ENVELOPE_ALLOWANCE),
            segments,
            index,
            removal,
            artifact_files,
            artifact_publications: PublicationRegistry::new(),
            artifact_cache_locks: CacheLocks::new(),
            retention,
            budget_state: StdMutex::new(RecordingBudgetState::Available),
            trim_exhausted: StdMutex::new(false),
            census,
            availability,
            #[cfg(test)]
            evidence_read_pause: StdMutex::new(None),
        };
        store.verify_recovery_completed()?;
        store.resume_deletions()?;
        store.recover_artifacts()?;
        store.recover_browser_events()?;
        Ok(store)
    }

    /// Enforces the caller invariant that `recover()` ran before construction.
    ///
    /// Recovery seals every `open` segment row it finds, so an `open` row at
    /// construction time can only mean a previous process left one behind and
    /// recovery was skipped. Continuing would let a writer allocate segments
    /// alongside unreconciled state, so this fails closed instead of documenting
    /// the ordering and hoping callers honour it.
    fn verify_recovery_completed(&self) -> krometrail_core::Result<()> {
        if self.index.open_segment_count()? != 0 {
            return Err(persistence_error(
                "recording store opened before segment recovery completed",
            ));
        }
        Ok(())
    }

    pub fn index(&self) -> &Arc<SqliteIndex> {
        &self.index
    }

    #[cfg(feature = "qualification-support")]
    pub(crate) fn qualification_inject_corrupt_ready_artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> krometrail_core::Result<()> {
        let final_path = self.artifact_files.final_path(artifact_id);
        if !final_path.is_file() {
            return Err(persistence_error(
                "qualification artifact fault requires a ready payload",
            ));
        }
        fs::write(
            self.artifact_files.temp_path(artifact_id),
            b"qualification staged artifact",
        )
        .map_err(|_| persistence_error("could not stage a qualification artifact fault"))?;
        fs::write(final_path, b"qualification corrupt artifact")
            .map_err(|_| persistence_error("could not corrupt a qualification artifact"))
    }

    #[cfg(feature = "qualification-support")]
    pub(crate) fn qualification_artifact_recovery_files_absent(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> krometrail_core::Result<bool> {
        Ok(!self.artifact_files.final_path(artifact_id).exists()
            && !self.artifact_files.temp_path(artifact_id).exists())
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

    fn recover_browser_events(&self) -> krometrail_core::Result<()> {
        let report = self.index.recover_browser_events()?;
        tracing::info!(
            timeline_rows_repaired = report.timeline_rows_repaired,
            usage_rows_repaired = report.usage_rows_repaired,
            corrupt_rows_discarded = report.corrupt_rows_discarded,
            orphan_timeline_rows_discarded = report.orphan_timeline_rows_discarded,
            orphan_usage_rows_removed = report.orphan_usage_rows_removed,
            "browser event recovery complete"
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

    #[cfg(test)]
    fn pause_next_evidence_read(&self, kind: EvidenceReadKind) -> Arc<EvidenceReadPause> {
        let pause = Arc::new(EvidenceReadPause {
            kind,
            reached: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        *self
            .evidence_read_pause
            .lock()
            .expect("evidence read pause lock poisoned") = Some(Arc::clone(&pause));
        pause
    }

    async fn pause_after_read_snapshot(&self, #[allow(unused_variables)] kind: EvidenceReadKind) {
        #[cfg(test)]
        {
            let pause = {
                let mut configured = self
                    .evidence_read_pause
                    .lock()
                    .expect("evidence read pause lock poisoned");
                if configured.as_ref().is_some_and(|pause| pause.kind == kind) {
                    configured.take()
                } else {
                    None
                }
            };
            if let Some(pause) = pause {
                pause.reached.notify_one();
                pause.resume.notified().await;
            }
        }
    }

    async fn read_frame_snapshots(
        &self,
        snapshots: Vec<FrameReadSnapshot>,
        kind: EvidenceReadKind,
    ) -> krometrail_core::Result<Vec<EncodedFrame>> {
        self.pause_after_read_snapshot(kind).await;
        let read = snapshots
            .iter()
            .map(|snapshot| self.index.read_frame_snapshot(snapshot))
            .collect::<krometrail_core::Result<Vec<_>>>();

        let _mutation = self.mutations.lock().await;
        for session_id in snapshots
            .iter()
            .map(|snapshot| snapshot.metadata.session_id())
            .collect::<HashSet<_>>()
        {
            self.reject_deleted(session_id)?;
        }
        let ids: Vec<_> = snapshots
            .iter()
            .map(|snapshot| snapshot.metadata.id())
            .collect();
        let current = self.index.frame_read_snapshots_by_id(&ids);
        if current.as_ref().ok() != Some(&snapshots) {
            return Err(source_read_not_found(snapshots.first()));
        }
        read
    }

    fn artifact_snapshot(&self, row: ArtifactRow) -> krometrail_core::Result<ArtifactReadSnapshot> {
        let sources = self.index.artifact_sources(row.artifact_id)?;
        let frames = self.index.frame_read_snapshots_by_id(
            &sources
                .iter()
                .map(|source| source.frame_id)
                .collect::<Vec<_>>(),
        )?;
        Ok(ArtifactReadSnapshot {
            row,
            sources,
            frames,
        })
    }

    async fn validate_source_payloads(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        expected: &[ArtifactSourceFingerprint],
    ) -> krometrail_core::Result<()> {
        let snapshots = {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(session_id)?;
            self.index.frame_read_snapshots_by_id(
                &expected
                    .iter()
                    .map(|source| source.frame_id)
                    .collect::<Vec<_>>(),
            )?
        };
        let frames = self
            .read_frame_snapshots(snapshots, EvidenceReadKind::Source)
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
            snapshot.usage.accounting_slack_bytes,
        )?;
        RetentionStatus::new(
            self.retention.budget(),
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

    fn progressive_pin_state_locked(
        &self,
        request: RetentionPinRequest,
    ) -> krometrail_core::Result<PinState> {
        let snapshot = self.index.progressive_pin_snapshot(&request)?;
        let coalesced = krometrail_core::coalesce_protected_ranges(&snapshot.protected_segments)?;
        let retention = self.current_status()?;
        PinState::new(
            request,
            snapshot.exact_pin_active,
            snapshot.evidence,
            PinProtectionScope::SourceSegmentsOnly,
            snapshot.protected_segments,
            coalesced,
            snapshot.pinned_usage_bytes,
            retention,
        )
    }

    async fn ensure_append_capacity(&self, frame: &EncodedFrame) -> krometrail_core::Result<()> {
        let required = frame
            .byte_len()
            .get()
            .checked_add(RECORD_ENVELOPE_ALLOWANCE)
            .ok_or_else(|| persistence_error("frame storage estimate overflow"))?;
        let snapshot = self.refresh_usage()?;
        let total = snapshot.usage.total_bytes()?;
        // One decision basis for the whole append. The share is an equal division
        // of the total, so the bytes about to be written need no separate
        // accounting: `total + required <= effective` already refuses a frame of
        // any size that would carry this instance past its share.
        let effective = self.effective_budget();
        if total
            .checked_add(required)
            .is_some_and(|needed| needed <= effective)
        {
            // The frame fits, but a long session should still trim as it goes
            // rather than climbing to the budget wall and staying there. Reclaim
            // already-sealed evidence back to the high-water mark, and age out
            // expired evidence even below it.
            self.trim_locked(total, effective).await?;
            return Ok(());
        }
        self.flush_all().await?;
        self.cleanup_to(effective.saturating_sub(required), None)
            .await?;
        let snapshot = self.refresh_usage()?;
        if snapshot
            .usage
            .total_bytes()?
            .checked_add(required)
            .is_some_and(|needed| needed <= effective)
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

    /// In-session trimming.
    ///
    /// Runs on the append path while the frame still fits, so reclaim happens
    /// during a live session instead of only at the budget wall. Trimming never
    /// flushes: it reclaims evidence that is already sealed, which keeps a hot
    /// append path from forcing segment rotation.
    ///
    /// A walk that reclaims nothing sets an exhaustion latch, so a store whose
    /// remaining evidence is entirely pinned does not re-walk on every frame.
    /// Any later reclamation clears the latch.
    async fn trim_locked(
        &self,
        total_bytes: u64,
        effective_budget: u64,
    ) -> krometrail_core::Result<()> {
        let high_water = self.retention.trim_high_water_bytes(effective_budget);
        let expired_pending = match self.expiry_cutoff()? {
            Some(cutoff) => self.index.expired_object_count(cutoff)? != 0,
            None => false,
        };
        if total_bytes < high_water && !expired_pending {
            return Ok(());
        }
        if self.trim_exhausted() && !expired_pending {
            return Ok(());
        }
        let outcome = self.reclaim(high_water, None).await?;
        if outcome.reclaimed_anything() {
            tracing::info!(
                event = "retention.trimmed",
                trigger = "in_session",
                segments = outcome.segments,
                artifacts = outcome.artifacts,
                browser_events = outcome.browser_events,
                bytes = outcome.bytes,
                high_water_bytes = high_water,
                "reclaimed retained evidence during a live session"
            );
        } else {
            self.set_trim_exhausted(true);
        }
        Ok(())
    }

    fn trim_exhausted(&self) -> bool {
        *self
            .trim_exhausted
            .lock()
            .expect("trim exhaustion lock poisoned")
    }

    fn set_trim_exhausted(&self, value: bool) {
        *self
            .trim_exhausted
            .lock()
            .expect("trim exhaustion lock poisoned") = value;
    }

    async fn enforce_locked(&self) -> krometrail_core::Result<RetentionStatus> {
        let mut snapshot = self.refresh_usage()?;
        if self.usage_is_within_budget(&snapshot)? {
            self.set_budget_state(RecordingBudgetState::Available);
            return self.status_from_snapshot(snapshot, RecordingBudgetState::Available);
        }
        self.flush_all().await?;
        self.cleanup_to(self.effective_budget(), None).await?;
        snapshot = self.refresh_usage()?;
        let state = if self.usage_is_within_budget(&snapshot)? {
            RecordingBudgetState::Available
        } else {
            RecordingBudgetState::PausedBudget
        };
        self.set_budget_state(state);
        self.status_from_snapshot(snapshot, state)
    }

    fn usage_is_within_budget(
        &self,
        snapshot: &crate::index::retention::UsageSnapshot,
    ) -> krometrail_core::Result<bool> {
        let total = snapshot.usage.total_bytes()?;
        let effective = self.effective_budget();
        if total <= effective {
            return Ok(true);
        }
        if snapshot.open_segment_count != 1
            || total.saturating_sub(effective) > self.open_overhead_limit
        {
            return Ok(false);
        }
        // The bounded open-segment allowance must not shelter older evictable
        // evidence. It applies only after artifact/event/sealed-segment cleanup
        // has no candidate left to reclaim.
        Ok(self.index.oldest_artifact()?.is_none()
            && self.index.oldest_unpinned_segment()?.is_none()
            && self.index.oldest_browser_event()?.is_none())
    }

    async fn cleanup_to(
        &self,
        target_bytes: u64,
        protected_artifact: Option<krometrail_core::ArtifactId>,
    ) -> krometrail_core::Result<()> {
        self.reclaim(target_bytes, protected_artifact)
            .await
            .map(|_| ())
    }

    /// This instance's byte allowance: an equal share of one total budget.
    ///
    /// The configured budget is a total across every live instance, so with `N`
    /// instances sharing a data directory each may hold `total / N`. Every write
    /// is judged against a count read at that moment, which makes the guarantee
    /// directly provable: no instance ever exceeds `total / N`, so once each
    /// instance has performed one operation since the newest one joined, the
    /// combined footprint is at most the total.
    ///
    /// This deliberately does not ask what peers are *using*. A policy shaped
    /// like `total - other_live_usage` would let a busy instance claim what idle
    /// peers are not holding, but it can only be honoured with each peer's exact
    /// byte count at the instant of a write — a figure that is stale the moment
    /// it is read, because instances write independently. Four review rounds
    /// found four defects in the machinery that tried, all of them the same
    /// shape. The accepted cost of the simpler policy is that two live instances
    /// each get `total / 2` even when one is idle. Predictability is the trade.
    ///
    /// Without a census — a single-instance store, or a host that cannot prove
    /// ownership — this is the configured budget, which is what a lone instance
    /// gets anyway.
    fn effective_budget(&self) -> u64 {
        let configured = self.retention.budget().get();
        self.census.as_ref().map_or(configured, |census| {
            configured / census.live_instances().max(1)
        })
    }

    /// Live instances currently dividing the total budget with this store.
    ///
    /// One without a census: a store outside a multi-instance data directory
    /// enforces the configured budget alone, which is the same thing as being
    /// the only live instance.
    ///
    /// This exists so that a caller asking how the budget is being divided reads
    /// the answer from the census this store actually enforces against, rather
    /// than standing up a second census over the same directory. A lookalike
    /// census is not the same object: it holds no instance lock, and it was
    /// precisely that divergence between the tested shape and the running one
    /// that let the ownership-lifetime defect through.
    pub fn live_instances(&self) -> u64 {
        self.census
            .as_ref()
            .map_or(1, |census| census.live_instances().max(1))
    }

    /// Age cutoff for the configured policy, read from the index's own clock.
    fn expiry_cutoff(&self) -> krometrail_core::Result<Option<i64>> {
        let Some(max_age) = self.retention.max_age() else {
            return Ok(None);
        };
        let millis = i64::try_from(max_age.as_millis()).unwrap_or(i64::MAX);
        Ok(Some(self.index.now_unix_ms()?.saturating_sub(millis)))
    }

    /// Instant before which a published artifact has left its grace window.
    fn artifact_grace_since(&self) -> krometrail_core::Result<Option<i64>> {
        let grace = self.retention.artifact_grace();
        if grace.is_zero() {
            return Ok(None);
        }
        let millis = i64::try_from(grace.as_millis()).unwrap_or(i64::MAX);
        Ok(Some(self.index.now_unix_ms()?.saturating_sub(millis)))
    }

    /// The single reclaim walk.
    ///
    /// Budget pressure, in-session trimming, and age-out all enter here; they
    /// differ only in the target and in how the candidate set is narrowed, never
    /// in ordering or in what pins protect. Keeping them on one walk is what
    /// stops age-out from becoming a second, subtly different eviction engine.
    ///
    /// Reclaim proceeds in tiers, cheapest loss first. Tier 0 is reserved for
    /// whole abandoned instance roots once per-instance isolation lands: nothing
    /// live references them, so they belong ahead of every tier here and slot in
    /// without reshaping the walk.
    async fn reclaim(
        &self,
        target_bytes: u64,
        protected_artifact: Option<krometrail_core::ArtifactId>,
    ) -> krometrail_core::Result<ReclaimOutcome> {
        let mut outcome = ReclaimOutcome::default();
        loop {
            let over_target = self.refresh_usage()?.usage.total_bytes()? > target_bytes;
            let filter = if over_target {
                SegmentReclaimFilter {
                    created_before_unix_ms: None,
                    artifact_grace_since_unix_ms: self.artifact_grace_since()?,
                }
            } else {
                // Inside the byte target, only expired evidence is reclaimable.
                // This is what stops a store from sitting pinned at ~99% forever.
                let Some(cutoff) = self.expiry_cutoff()? else {
                    return Ok(outcome);
                };
                if self.index.expired_object_count(cutoff)? == 0 {
                    return Ok(outcome);
                }
                SegmentReclaimFilter {
                    created_before_unix_ms: Some(cutoff),
                    artifact_grace_since_unix_ms: None,
                }
            };
            if !self
                .reclaim_once(filter, over_target, protected_artifact, &mut outcome)
                .await?
            {
                return Ok(outcome);
            }
        }
    }

    /// Reclaims one object, returning whether progress was made.
    async fn reclaim_once(
        &self,
        filter: SegmentReclaimFilter,
        under_pressure: bool,
        protected_artifact: Option<krometrail_core::ArtifactId>,
        outcome: &mut ReclaimOutcome,
    ) -> krometrail_core::Result<bool> {
        // Derived artifacts go first: they are regenerable, so they are the
        // cheapest evidence to lose.
        if let Some(artifact) = self
            .index
            .oldest_reclaimable_artifact(protected_artifact, filter.created_before_unix_ms)?
        {
            let (_, _, artifacts, bytes) = self
                .remove_objects(
                    DeletionKind::Eviction,
                    None,
                    vec![artifact_object(artifact)],
                )
                .await?;
            outcome.artifacts = outcome.artifacts.saturating_add(artifacts);
            outcome.bytes = outcome.bytes.saturating_add(bytes);
            return Ok(true);
        }

        let mut segment = self.index.oldest_reclaimable_segment(filter)?;
        if segment.is_none() && filter.artifact_grace_since_unix_ms.is_some() {
            // Every remaining segment backs an artifact still inside its grace
            // window. Liveness wins over the grace promise: stalling capture at a
            // full store is worse than losing a fresh evidence link, so the grace
            // is dropped and the override is reported rather than hidden.
            segment = self
                .index
                .oldest_reclaimable_segment(SegmentReclaimFilter {
                    artifact_grace_since_unix_ms: None,
                    ..filter
                })?;
            if segment.is_some() {
                tracing::info!(
                    event = "retention.artifact_grace_overridden",
                    "budget pressure evicted a segment backing a recently published artifact"
                );
                outcome.artifact_grace_overridden = true;
            }
        }

        let event = self.index.oldest_browser_event()?;
        let event_is_older = match (&segment, event) {
            (_, None) => false,
            // Without a bounding segment, unbounded event eviction is only
            // correct under pressure. Age-out must not reach events it has not
            // proven to be expired.
            (None, Some(_)) => under_pressure,
            (Some(segment), Some(event)) => event.retention_sequence < segment.retention_sequence,
        };
        if event_is_older {
            let before = segment.as_ref().map(|segment| segment.retention_sequence);
            let removed = self.index.evict_oldest_browser_events(before)?;
            outcome.browser_events = outcome.browser_events.saturating_add(removed);
            return Ok(removed != 0);
        }

        let Some(segment) = segment else {
            return Ok(false);
        };
        let mut objects: Vec<_> = self
            .index
            .artifacts_for_segment(segment.segment_id)?
            .into_iter()
            .map(artifact_object)
            .collect();
        objects.push(segment_object(segment));
        let (segments, _, artifacts, bytes) = self
            .remove_objects(DeletionKind::Eviction, None, objects)
            .await?;
        outcome.segments = outcome.segments.saturating_add(segments);
        outcome.artifacts = outcome.artifacts.saturating_add(artifacts);
        outcome.bytes = outcome.bytes.saturating_add(bytes);
        Ok(true)
    }

    async fn ensure_staged_artifact_capacity(
        &self,
        row: &ArtifactRow,
    ) -> krometrail_core::Result<()> {
        self.cleanup_to(self.effective_budget(), Some(row.artifact_id))
            .await?;
        if self.refresh_usage()?.usage.total_bytes()? <= self.effective_budget() {
            self.set_budget_state(RecordingBudgetState::Available);
            return Ok(());
        }
        self.set_budget_state(RecordingBudgetState::PausedBudget);
        Err(budget_error(row.session_id, row.target_id))
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
        self.set_trim_exhausted(false);
        let mut committed = batch.clone();
        committed.state = DeletionState::MetadataRemoved;
        self.removal.finalize(committed).await?;
        self.index.finalize_deletion(&batch)?;
        Ok((segments, frames, artifacts, removed_bytes))
    }

    async fn progressive_source_reads(
        &self,
        request: SourceFramesRequest,
    ) -> krometrail_core::Result<Vec<SourceFrameRead>> {
        let selected_ids = request.selected_frame_ids();
        let snapshots = {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.range.session_id)?;
            self.index.frame_read_snapshots_by_id(&selected_ids)?
        };
        if snapshots.len() != selected_ids.len() {
            return Err(source_read_not_found(snapshots.first()));
        }
        for (expected, snapshot) in selected_ids.iter().zip(&snapshots) {
            if snapshot.metadata.id() != *expected
                || snapshot.metadata.session_id() != request.range.session_id
                || snapshot.metadata.target_id() != request.range.target_id
            {
                return Err(source_read_not_found(Some(snapshot)));
            }
        }
        if matches!(request.selection, SourceFrameSelection::ResolvedOrder)
            && snapshots.windows(2).any(|pair| {
                pair[0].metadata.capture_ordinal() >= pair[1].metadata.capture_ordinal()
            })
        {
            return Err(persistence_error(
                "resolved source frame order is not strict capture order",
            ));
        }

        self.pause_after_read_snapshot(EvidenceReadKind::Source)
            .await;
        let frames = snapshots
            .iter()
            .map(|snapshot| self.index.read_frame_snapshot(snapshot))
            .collect::<krometrail_core::Result<Vec<_>>>();
        // Payload bounds, hashes, and handles are completed outside the mutation
        // gate. No partial result escapes if any item fails.
        let prepared = frames.and_then(|frames| {
            let mut total_bytes = 0_u64;
            let scope = EvidenceScope::from_range(&request.range)?;
            let mut reads = Vec::with_capacity(frames.len());
            for (request_position, frame) in frames.into_iter().enumerate() {
                let encoded_byte_len = frame.byte_len().get();
                if encoded_byte_len > request.limits.max_item_bytes() {
                    return Err(source_limit_error(
                        "source frame exceeds the per-item encoded-byte limit",
                        scope,
                    ));
                }
                total_bytes = total_bytes
                    .checked_add(encoded_byte_len)
                    .ok_or_else(|| source_limit_error("source frame byte total overflow", scope))?;
                if total_bytes > request.limits.max_total_bytes() {
                    return Err(source_limit_error(
                        "source frames exceed the total encoded-byte limit",
                        scope,
                    ));
                }
                let resolved_position = request
                    .range
                    .frame_ids
                    .iter()
                    .position(|id| *id == frame.metadata().id())
                    .ok_or_else(|| source_read_not_found(None))?;
                let media_type = match frame.metadata().format() {
                    krometrail_core::ImageFormat::Jpeg => "image/jpeg",
                    krometrail_core::ImageFormat::Png => "image/png",
                };
                let handle = SourceFrameHandle::new(
                    frame.metadata().id(),
                    scope,
                    u32::try_from(request_position)
                        .map_err(|_| persistence_error("source request position overflow"))?,
                    u32::try_from(resolved_position)
                        .map_err(|_| persistence_error("source resolved position overflow"))?,
                    NonEmptyText::new(media_type).expect("source media type is non-empty"),
                    Sha256Digest::digest(frame.bytes()),
                    encoded_byte_len,
                    frame.metadata().clone(),
                )?;
                reads.push(SourceFrameRead::new(handle, frame.encoded_bytes())?);
            }
            Ok(reads)
        });

        let _mutation = self.mutations.lock().await;
        self.reject_deleted(request.range.session_id)?;
        let current = self.index.frame_read_snapshots_by_id(&selected_ids);
        if current.as_ref().ok() != Some(&snapshots) {
            return Err(source_read_not_found(snapshots.first()));
        }
        prepared
    }

    async fn progressive_source_frame_read(
        &self,
        request: RetrieveSourceFrameRequest,
    ) -> krometrail_core::Result<SourceFrameRead> {
        let snapshot = {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.scope.session_id)?;
            let snapshots = match self.index.frame_read_snapshots_by_id(&[request.frame_id]) {
                Ok(snapshots) => snapshots,
                Err(error) if error.code == ErrorCode::NotFound => {
                    return Err(source_read_not_found_for_scope(request.scope));
                }
                Err(error) => return Err(error),
            };
            let Some(snapshot) = snapshots.into_iter().next() else {
                return Err(source_read_not_found_for_scope(request.scope));
            };
            if snapshot.metadata.id() != request.frame_id
                || snapshot.metadata.session_id() != request.scope.session_id
                || snapshot.metadata.target_id() != request.scope.target_id
            {
                return Err(source_read_not_found_for_scope(request.scope));
            }
            snapshot
        };

        self.pause_after_read_snapshot(EvidenceReadKind::Source)
            .await;
        let frame_result = self.index.read_frame_snapshot(&snapshot);
        {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.scope.session_id)?;
            let current = self
                .index
                .frame_read_snapshots_by_id(&[request.frame_id])
                .ok()
                .and_then(|mut snapshots| snapshots.pop());
            if current.as_ref() != Some(&snapshot) {
                return Err(source_read_not_found_for_scope(request.scope));
            }
        }
        let frame = frame_result?;
        if frame.byte_len().get() > request.max_encoded_bytes() {
            return Err(source_limit_error(
                "source frame exceeds the encoded-byte limit",
                request.scope,
            ));
        }
        let media_type = match frame.metadata().format() {
            krometrail_core::ImageFormat::Jpeg => "image/jpeg",
            krometrail_core::ImageFormat::Png => "image/png",
        };
        let handle = SourceFrameHandle::new(
            frame.metadata().id(),
            request.scope,
            0,
            0,
            NonEmptyText::new(media_type).expect("source media type is non-empty"),
            Sha256Digest::digest(frame.bytes()),
            frame.byte_len().get(),
            frame.metadata().clone(),
        )?;
        let prepared = SourceFrameRead::new(handle, frame.encoded_bytes())?;

        let _mutation = self.mutations.lock().await;
        self.reject_deleted(request.scope.session_id)?;
        let current = self
            .index
            .frame_read_snapshots_by_id(&[request.frame_id])
            .ok()
            .and_then(|mut snapshots| snapshots.pop());
        if current.as_ref() != Some(&snapshot) {
            return Err(source_read_not_found_for_scope(request.scope));
        }
        Ok(prepared)
    }

    async fn read_artifact_snapshot(
        &self,
        snapshot: &ArtifactReadSnapshot,
        expected_sources: Option<&[ArtifactSourceFingerprint]>,
    ) -> krometrail_core::Result<RetainedStoredArtifact> {
        self.pause_after_read_snapshot(EvidenceReadKind::Artifact)
            .await;
        let source_frames = snapshot
            .frames
            .iter()
            .map(|frame| self.index.read_frame_snapshot(frame))
            .collect::<krometrail_core::Result<Vec<_>>>();
        let artifact_bytes = self
            .artifact_files
            .read(snapshot.row.relative_path.clone())
            .await;

        // All file reads and hashes complete before final metadata revalidation.
        let validated = match (source_frames, artifact_bytes) {
            (Err(error), _) => Err(error),
            (Ok(source_frames), _) if source_frames.len() != snapshot.sources.len() => Err(
                persistence_error("artifact source payload count contradicts retained links"),
            ),
            (Ok(source_frames), Ok(bytes)) => {
                if source_frames
                    .iter()
                    .zip(&snapshot.sources)
                    .any(|(frame, source)| {
                        frame.metadata().id() != source.frame_id
                            || frame.metadata().session_id() != snapshot.row.session_id
                            || frame.metadata().target_id() != snapshot.row.target_id
                    })
                {
                    Err(persistence_error(
                        "artifact source payloads contradict retained source metadata",
                    ))
                } else if source_frames
                    .iter()
                    .zip(&snapshot.sources)
                    .any(|(frame, source)| {
                        <[u8; 32]>::from(Sha256::digest(frame.bytes())) != source.encoded_hash
                    })
                {
                    Err(evidence_invalidated_error(
                        snapshot.row.session_id,
                        snapshot.row.target_id,
                    ))
                } else {
                    validate_stored_artifact(
                        &snapshot.row,
                        &snapshot.sources,
                        bytes,
                        expected_sources,
                    )
                    .map_err(|_| {
                        evidence_invalidated_error(snapshot.row.session_id, snapshot.row.target_id)
                    })
                }
            }
            (Ok(_), Err(_)) => Err(evidence_invalidated_error(
                snapshot.row.session_id,
                snapshot.row.target_id,
            )),
        };

        let _mutation = self.mutations.lock().await;
        self.reject_deleted(snapshot.row.session_id)?;
        let current = self
            .index
            .artifact_row(snapshot.row.artifact_id)?
            .filter(|row| row.state == ArtifactState::Ready)
            .map(|row| self.artifact_snapshot(row))
            .transpose()?;
        if current.as_ref() != Some(snapshot) {
            return Err(artifact_not_found(
                snapshot.row.session_id,
                snapshot.row.target_id,
            ));
        }
        validated
    }

    async fn invalidate_artifact_snapshot(
        &self,
        snapshot: &ArtifactReadSnapshot,
    ) -> krometrail_core::Result<()> {
        let _mutation = self.mutations.lock().await;
        let current = self
            .index
            .artifact_row(snapshot.row.artifact_id)?
            .map(|row| self.artifact_snapshot(row))
            .transpose()?;
        if current.as_ref() == Some(snapshot) {
            self.invalidate_artifact_row(snapshot.row.clone()).await?;
        }
        Ok(())
    }
}

impl FrameSource for RecordingStore {
    fn list_source_frames(
        &self,
        request: SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<SourceFrameList>> {
        Box::pin(async move {
            let range = request.range.clone();
            let total_selected = request.selection.selected_count(&range);
            let offset = request.offset;
            let omitted_frame_count = request.omitted_frame_count();
            let frames = self
                .progressive_source_reads(request)
                .await?
                .into_iter()
                .map(|read| read.handle)
                .collect::<Vec<_>>();
            let returned_page_len = frames.len();
            let next_offset = (offset as usize)
                .saturating_add(returned_page_len)
                .lt(&total_selected)
                .then(|| {
                    offset.saturating_add(u32::try_from(returned_page_len).unwrap_or(u32::MAX))
                });
            Ok(SourceFrameList {
                range,
                frames,
                omitted_frame_count,
                next_offset,
            })
        })
    }

    fn fetch_source_frames(
        &self,
        request: SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<SourceFrameBatch>> {
        Box::pin(async move {
            let range = request.range.clone();
            let frames = self.progressive_source_reads(request).await?;
            Ok(SourceFrameBatch { range, frames })
        })
    }

    fn read_source_frame(
        &self,
        request: RetrieveSourceFrameRequest,
    ) -> PortFuture<'_, krometrail_core::Result<SourceFrameRead>> {
        Box::pin(self.progressive_source_frame_read(request))
    }

    fn frames_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            let snapshots = {
                let _mutation = self.mutations.lock().await;
                self.index.frame_read_snapshots_by_id(&frame_ids)?
            };
            self.read_frame_snapshots(snapshots, EvidenceReadKind::Source)
                .await
        })
    }

    fn frame_metadata_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<krometrail_core::CapturedFrame>>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            let snapshots = self.index.frame_read_snapshots_by_id(&frame_ids)?;
            for snapshot in &snapshots {
                self.reject_deleted(snapshot.metadata.session_id())?;
            }
            Ok(snapshots
                .into_iter()
                .map(|snapshot| snapshot.metadata)
                .collect())
        })
    }

    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            let snapshots = {
                let _mutation = self.mutations.lock().await;
                self.reject_deleted(session_id)?;
                self.index
                    .frame_read_snapshots_in_range(session_id, target_id, range)?
            };
            self.read_frame_snapshots(snapshots, EvidenceReadKind::Source)
                .await
        })
    }

    fn frames_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: krometrail_core::CaptureOrdinal,
        end: krometrail_core::CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        Box::pin(async move {
            let snapshots = {
                let _mutation = self.mutations.lock().await;
                self.reject_deleted(session_id)?;
                self.index
                    .frame_read_snapshots_in_ordinal_range(session_id, target_id, start, end)?
            };
            self.read_frame_snapshots(snapshots, EvidenceReadKind::Source)
                .await
        })
    }

    fn frame_metadata_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<krometrail_core::CapturedFrame>>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(session_id)?;
            Ok(self
                .index
                .frame_read_snapshots_in_range(session_id, target_id, range)?
                .into_iter()
                .map(|snapshot| snapshot.metadata)
                .collect())
        })
    }

    fn frame_metadata_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: krometrail_core::CaptureOrdinal,
        end: krometrail_core::CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<krometrail_core::CapturedFrame>>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(session_id)?;
            Ok(self
                .index
                .frame_read_snapshots_in_ordinal_range(session_id, target_id, start, end)?
                .into_iter()
                .map(|snapshot| snapshot.metadata)
                .collect())
        })
    }

    fn frame_availability(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::FrameAvailability>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(session_id)?;
            FrameSource::frame_availability(self.index.as_ref(), session_id, target_id).await
        })
    }
}

impl ArtifactStore for RecordingStore {
    fn read_artifact(
        &self,
        request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactReadLookup>> {
        Box::pin(async move {
            let snapshot = {
                let _mutation = self.mutations.lock().await;
                self.reject_deleted(request.scope.session_id)?;
                let row = self.index.artifact_row(request.artifact_id)?.filter(|row| {
                    row.state == ArtifactState::Ready
                        && matches!(row.kind, RetainedArtifactKind::Image(_))
                        && row.session_id == request.scope.session_id
                        && row.target_id == request.scope.target_id
                });
                row.map(|row| self.artifact_snapshot(row)).transpose()?
            };
            let Some(snapshot) = snapshot else {
                return Ok(ArtifactReadLookup::Missing);
            };
            if snapshot.row.byte_len > request.max_encoded_bytes() {
                return Err(artifact_limit_error(request.scope));
            }
            match self.read_artifact_snapshot(&snapshot, None).await {
                Ok(RetainedStoredArtifact::Image(stored)) => {
                    let digest = Sha256Digest::from_bytes(snapshot.row.output_hash);
                    let handle = ArtifactEvidenceHandle::new(
                        snapshot.row.artifact_id,
                        request.scope,
                        stored.media_type.clone(),
                        digest,
                        snapshot.row.byte_len,
                        stored.manifest.clone(),
                    )?;
                    Ok(ArtifactReadLookup::Available(Box::new(ArtifactRead::new(
                        handle,
                        Arc::clone(&stored.encoded_bytes),
                    )?)))
                }
                Ok(RetainedStoredArtifact::Video(_)) => Ok(ArtifactReadLookup::Missing),
                Err(error) if error.code == ErrorCode::NotFound => Ok(ArtifactReadLookup::Missing),
                Err(error) if error.code == ErrorCode::EvidenceInvalidated => {
                    self.invalidate_artifact_snapshot(&snapshot).await?;
                    Ok(ArtifactReadLookup::Invalidated)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn lookup_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactLookup>> {
        Box::pin(async move {
            let snapshot = {
                let _mutation = self.mutations.lock().await;
                self.index
                    .artifact_by_cache(key, true)?
                    .filter(|row| matches!(row.kind, RetainedArtifactKind::Image(_)))
                    .map(|row| self.artifact_snapshot(row))
                    .transpose()?
            };
            let Some(snapshot) = snapshot else {
                return Ok(ArtifactLookup::Miss);
            };
            match self
                .read_artifact_snapshot(&snapshot, Some(&expected_sources))
                .await
            {
                Ok(RetainedStoredArtifact::Image(artifact)) => Ok(ArtifactLookup::Hit(artifact)),
                Ok(RetainedStoredArtifact::Video(_)) => Ok(ArtifactLookup::Miss),
                Err(error) if error.code == ErrorCode::NotFound => Ok(ArtifactLookup::Miss),
                Err(error) if error.code == ErrorCode::EvidenceInvalidated => {
                    self.invalidate_artifact_snapshot(&snapshot).await?;
                    Ok(ArtifactLookup::Invalidated)
                }
                Err(error) => Err(error),
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
                    let snapshot = {
                        let _mutation = self.mutations.lock().await;
                        self.artifact_snapshot(existing)?
                    };
                    let stored = self
                        .read_artifact_snapshot(&snapshot, Some(&publication.sources))
                        .await?;
                    let RetainedStoredArtifact::Image(stored) = stored else {
                        return Err(persistence_error(
                            "image cache key resolved to a temporal video artifact",
                        ));
                    };
                    return Ok(ArtifactPublish::Existing(*stored));
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
                    row.relative_path.clone(),
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
            let ready = {
                let _mutation = self.mutations.lock().await;
                let row = self
                    .index
                    .artifact_row(*publication.manifest.artifact_id())?
                    .ok_or_else(|| persistence_error("ready artifact metadata disappeared"))?;
                self.artifact_snapshot(row)?
            };
            let stored = self
                .read_artifact_snapshot(&ready, Some(&publication.sources))
                .await?;
            let RetainedStoredArtifact::Image(stored) = stored else {
                return Err(persistence_error(
                    "published image resolved to a temporal video artifact",
                ));
            };
            Ok(ArtifactPublish::Published(*stored))
        })
    }

    fn artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredArtifact>>> {
        Box::pin(async move {
            let snapshot = {
                let _mutation = self.mutations.lock().await;
                self.index
                    .artifact_row(artifact_id)?
                    .filter(|row| {
                        row.state == ArtifactState::Ready
                            && matches!(row.kind, RetainedArtifactKind::Image(_))
                    })
                    .map(|row| self.artifact_snapshot(row))
                    .transpose()?
            };
            let Some(snapshot) = snapshot else {
                return Ok(None);
            };
            match self.read_artifact_snapshot(&snapshot, None).await {
                Ok(RetainedStoredArtifact::Image(artifact)) => Ok(Some(*artifact)),
                Ok(RetainedStoredArtifact::Video(_)) => Ok(None),
                Err(error) if error.code == ErrorCode::NotFound => Ok(None),
                Err(error) if error.code == ErrorCode::EvidenceInvalidated => {
                    self.invalidate_artifact_snapshot(&snapshot).await?;
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn read_video_artifact(
        &self,
        request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, krometrail_core::Result<VideoArtifactReadLookup>> {
        Box::pin(async move {
            let snapshot = {
                let _mutation = self.mutations.lock().await;
                self.reject_deleted(request.scope.session_id)?;
                self.index
                    .artifact_row(request.artifact_id)?
                    .filter(|row| {
                        row.state == ArtifactState::Ready
                            && row.kind == RetainedArtifactKind::TemporalVideo
                            && row.session_id == request.scope.session_id
                            && row.target_id == request.scope.target_id
                    })
                    .map(|row| self.artifact_snapshot(row))
                    .transpose()?
            };
            let Some(snapshot) = snapshot else {
                return Ok(VideoArtifactReadLookup::Missing);
            };
            if snapshot.row.byte_len > request.max_encoded_bytes() {
                return Err(artifact_limit_error(request.scope));
            }
            match self.read_artifact_snapshot(&snapshot, None).await {
                Ok(RetainedStoredArtifact::Video(stored)) => {
                    let stored = *stored;
                    let handle = VideoArtifactEvidenceHandle::new(
                        snapshot.row.artifact_id,
                        request.scope,
                        NonEmptyText::new("video/mp4").expect("video media type is non-empty"),
                        Sha256Digest::from_bytes(snapshot.row.output_hash),
                        snapshot.row.byte_len,
                        stored.manifest,
                    )?;
                    Ok(VideoArtifactReadLookup::Available(Box::new(
                        VideoArtifactRead::new(handle, stored.encoded_bytes)?,
                    )))
                }
                Ok(RetainedStoredArtifact::Image(_)) => Ok(VideoArtifactReadLookup::Missing),
                Err(error) if error.code == ErrorCode::NotFound => {
                    Ok(VideoArtifactReadLookup::Missing)
                }
                Err(error) if error.code == ErrorCode::EvidenceInvalidated => {
                    self.invalidate_artifact_snapshot(&snapshot).await?;
                    Ok(VideoArtifactReadLookup::Invalidated)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn lookup_video_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<VideoArtifactLookup>> {
        Box::pin(async move {
            let snapshot = {
                let _mutation = self.mutations.lock().await;
                self.index
                    .artifact_by_cache(key, true)?
                    .filter(|row| row.kind == RetainedArtifactKind::TemporalVideo)
                    .map(|row| self.artifact_snapshot(row))
                    .transpose()?
            };
            let Some(snapshot) = snapshot else {
                return Ok(VideoArtifactLookup::Miss);
            };
            match self
                .read_artifact_snapshot(&snapshot, Some(&expected_sources))
                .await
            {
                Ok(RetainedStoredArtifact::Video(artifact)) => {
                    Ok(VideoArtifactLookup::Hit(artifact))
                }
                Ok(RetainedStoredArtifact::Image(_)) => Ok(VideoArtifactLookup::Miss),
                Err(error) if error.code == ErrorCode::NotFound => Ok(VideoArtifactLookup::Miss),
                Err(error) if error.code == ErrorCode::EvidenceInvalidated => {
                    self.invalidate_artifact_snapshot(&snapshot).await?;
                    Ok(VideoArtifactLookup::Invalidated)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn publish_video_artifact(
        &self,
        publication: VideoArtifactPublication,
    ) -> PortFuture<'_, krometrail_core::Result<VideoArtifactPublish>> {
        Box::pin(async move {
            let publication_guard = self.artifact_publications.begin(publication.session_id)?;
            let cache_lock = self
                .artifact_cache_locks
                .for_key(publication.cache.cache_key);
            let _cache = cache_lock.lock().await;

            match self
                .lookup_video_artifact(publication.cache.cache_key, publication.sources.clone())
                .await?
            {
                VideoArtifactLookup::Hit(artifact) => {
                    return Ok(VideoArtifactPublish::Existing(*artifact));
                }
                VideoArtifactLookup::Miss | VideoArtifactLookup::Invalidated => {}
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
                self.index.stage_video_artifact(&publication)?
            };
            let row = match staged {
                StageArtifact::Staged(row) => row,
                StageArtifact::Existing(existing) if existing.state == ArtifactState::Ready => {
                    let snapshot = {
                        let _mutation = self.mutations.lock().await;
                        self.artifact_snapshot(existing)?
                    };
                    let stored = self
                        .read_artifact_snapshot(&snapshot, Some(&publication.sources))
                        .await?;
                    let RetainedStoredArtifact::Video(stored) = stored else {
                        return Err(persistence_error(
                            "video cache key resolved to an image artifact",
                        ));
                    };
                    return Ok(VideoArtifactPublish::Existing(*stored));
                }
                StageArtifact::Existing(existing) => {
                    let _mutation = self.mutations.lock().await;
                    self.invalidate_artifact_row(existing).await?;
                    return Err(persistence_error(
                        "stale video staging state was invalidated; retry publication",
                    ));
                }
            };

            {
                let _mutation = self.mutations.lock().await;
                if let Err(error) = self.ensure_staged_artifact_capacity(&row).await {
                    self.invalidate_artifact_row(row.clone()).await?;
                    return Err(error);
                }
            }
            if let Err(error) = self
                .validate_source_payloads(
                    publication.session_id,
                    publication.target_id,
                    &publication.sources,
                )
                .await
            {
                let _mutation = self.mutations.lock().await;
                self.invalidate_artifact_row(row.clone()).await?;
                return Err(error);
            }

            if let Err(error) = self
                .artifact_files
                .publish(
                    row.artifact_id,
                    row.relative_path.clone(),
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
                if let Err(error) = self.ensure_staged_artifact_capacity(&row).await {
                    self.invalidate_artifact_row(row.clone()).await?;
                    return Err(error);
                }
                if publication_guard.is_cancelled()
                    || publication
                        .cancellation()
                        .is_some_and(|signal| signal.is_cancelled())
                    || self.is_deleted(publication.session_id)
                {
                    self.invalidate_artifact_row(row.clone()).await?;
                    return Err(cancelled_publication_error());
                }
                match self.index.finalize_artifact(
                    row.artifact_id,
                    publication.cache.cache_key,
                    publication.session_id,
                    publication.target_id,
                    &publication.sources,
                ) {
                    Ok(finalized) => finalized,
                    Err(error) => {
                        self.invalidate_artifact_row(row.clone()).await?;
                        return Err(error);
                    }
                }
            };
            if !finalized {
                let _mutation = self.mutations.lock().await;
                self.invalidate_artifact_row(row).await?;
                return Err(persistence_error(
                    "video publication did not reach ready state",
                ));
            }
            let ready = {
                let _mutation = self.mutations.lock().await;
                let row = self
                    .index
                    .artifact_row(publication.manifest.artifact_id())?
                    .ok_or_else(|| persistence_error("ready video metadata disappeared"))?;
                self.artifact_snapshot(row)?
            };
            let stored = self
                .read_artifact_snapshot(&ready, Some(&publication.sources))
                .await?;
            let RetainedStoredArtifact::Video(stored) = stored else {
                return Err(persistence_error("published video resolved to an image"));
            };
            Ok(VideoArtifactPublish::Published(*stored))
        })
    }

    fn video_artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredVideoArtifact>>> {
        Box::pin(async move {
            let snapshot = {
                let _mutation = self.mutations.lock().await;
                self.index
                    .artifact_row(artifact_id)?
                    .filter(|row| {
                        row.state == ArtifactState::Ready
                            && row.kind == RetainedArtifactKind::TemporalVideo
                    })
                    .map(|row| self.artifact_snapshot(row))
                    .transpose()?
            };
            let Some(snapshot) = snapshot else {
                return Ok(None);
            };
            match self.read_artifact_snapshot(&snapshot, None).await {
                Ok(RetainedStoredArtifact::Video(artifact)) => Ok(Some(*artifact)),
                Ok(RetainedStoredArtifact::Image(_)) => Ok(None),
                Err(error) if error.code == ErrorCode::NotFound => Ok(None),
                Err(error) if error.code == ErrorCode::EvidenceInvalidated => {
                    self.invalidate_artifact_snapshot(&snapshot).await?;
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn invalidate_video_artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            let Some(row) = self.index.artifact_row(artifact_id)? else {
                return Ok(());
            };
            if row.kind != RetainedArtifactKind::TemporalVideo {
                return Err(persistence_error(
                    "video invalidation resolved to a non-video artifact",
                ));
            }
            self.invalidate_artifact_row(row).await
        })
    }
}

impl BrowserEventSink for RecordingStore {
    fn append_event_batch(
        &self,
        batch: BrowserEventBatch,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(batch.session_id())?;
            // Measure before the immediate transaction so its budget decision starts
            // from checkpointed live-page accounting. The transaction then considers
            // only bounded inserts and bounded event-only evictions.
            let managed_usage = self.refresh_usage()?.usage.total_bytes()?;
            self.index
                .append_browser_event_batch(batch, self.effective_budget(), managed_usage)
        })
    }
}

impl BrowserEventSource for RecordingStore {
    fn count_events(
        &self,
        selector: BrowserEventSelector,
    ) -> PortFuture<'_, krometrail_core::Result<u64>> {
        BrowserEventSource::count_events(self.index.as_ref(), selector)
    }

    fn chronological_events(
        &self,
        selector: BrowserEventSelector,
        cursor: Option<BrowserEventCursor>,
        limit: EventPageLimit,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<krometrail_core::BrowserEvent>>> {
        BrowserEventSource::chronological_events(self.index.as_ref(), selector, cursor, limit)
    }

    fn priority_candidates(
        &self,
        selector: BrowserEventSelector,
        limit: EventCandidateLimit,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<krometrail_core::BrowserEvent>>> {
        BrowserEventSource::priority_candidates(self.index.as_ref(), selector, limit)
    }

    fn nearest_candidates(
        &self,
        selector: BrowserEventSelector,
        focus_times: Vec<SessionTime>,
        each_side: u8,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<krometrail_core::BrowserEvent>>> {
        BrowserEventSource::nearest_candidates(
            self.index.as_ref(),
            selector,
            focus_times,
            each_side,
        )
    }

    fn unavailable_ranges(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserEventUnavailableRange>>> {
        BrowserEventSource::unavailable_ranges(
            self.index.as_ref(),
            session_id,
            target_id,
            range,
            limit,
        )
    }

    fn capture_status_samples(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, krometrail_core::Result<CaptureStatusSamples>> {
        BrowserEventSource::capture_status_samples(
            self.index.as_ref(),
            session_id,
            target_id,
            range,
            limit,
        )
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
            let commit = self
                .segments
                .append_indexable(frame.clone())
                .await
                .map_err(|error| classify_sink_failure(PersistenceOperation::FrameIndex, error))?;
            let mut connection = self
                .index
                .connection()
                .map_err(|error| classify_sink_failure(PersistenceOperation::FrameIndex, error))?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| persistence_error("could not begin indexed frame persistence"))
                .map_err(|error| classify_sink_failure(PersistenceOperation::FrameIndex, error))?;
            index_frame_tx(&transaction, &frame, &commit)
                .map_err(|error| classify_sink_failure(PersistenceOperation::FrameIndex, error))?;
            transaction
                .commit()
                .map_err(|_| persistence_error("could not commit indexed frame metadata"))
                .map_err(|error| classify_sink_failure(PersistenceOperation::FrameIndex, error))?;
            Ok(commit.address)
        })
    }

    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(gap.session_id())?;
            CaptureGapStore::append_gap(self.index.as_ref(), gap)
                .await
                .map_err(|error| classify_sink_failure(PersistenceOperation::GapIndex, error))
        })
    }

    fn flush(&self, session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(session_id)?;
            self.flush_session(session_id).await.map_err(|error| {
                classify_sink_failure(PersistenceOperation::SessionFlush, error)
            })?;
            self.enforce_locked()
                .await
                .map(|_| ())
                .map_err(|error| classify_sink_failure(PersistenceOperation::SessionFlush, error))
        })
    }
}

fn classify_sink_failure(
    operation: krometrail_core::PersistenceOperation,
    error: KrometrailError,
) -> KrometrailError {
    if error.code != ErrorCode::PersistenceFailed || error.persistence.is_some() {
        return error;
    }
    error.with_persistence(krometrail_core::PersistenceFailure::new(
        operation,
        krometrail_core::PersistenceFailureCategory::Other,
        krometrail_core::PersistenceRecoverability::WriterTerminal,
    ))
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

    fn selected_range(
        &self,
        query: krometrail_core::TimelineRangeQuery,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::TimelineRangeSlice>> {
        TimelineStore::selected_range(self.index.as_ref(), query)
    }
}

// Marker/interaction evidence reads are metadata-only and do not hold the
// mutation gate; the bundle service completes them before visual work begins.
impl InteractionAnchorSource for RecordingStore {
    fn interaction_anchor(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        InteractionAnchorSource::interaction_anchor(self.index.as_ref(), interaction_id)
    }

    fn latest_interaction_anchor(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        InteractionAnchorSource::latest_interaction_anchor(
            self.index.as_ref(),
            session_id,
            target_id,
        )
    }
}

impl InteractionRecordSource for RecordingStore {
    fn interaction_record(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionRecord>>> {
        InteractionRecordSource::interaction_record(self.index.as_ref(), interaction_id)
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
                Arc::clone(&self.clock),
            ))
            .resolve_range(request)
            .await
        })
    }
}

impl TemporalContextQuery for RecordingStore {
    fn context(
        &self,
        request: TemporalContextRequest,
    ) -> PortFuture<'_, krometrail_core::Result<TemporalContext>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.range().session_id)
                .map_err(|_| context_query_not_found(request.range()))?;
            TemporalContextService::new(Arc::clone(&self.index), Arc::clone(&self.index))
                .context(request)
                .await
        })
    }

    fn capture_quality(
        &self,
        range: ResolvedRange,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::CaptureQuality>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(range.session_id)
                .map_err(|_| context_query_not_found(&range))?;
            TemporalContextService::new(Arc::clone(&self.index), Arc::clone(&self.index))
                .capture_quality(range)
                .await
        })
    }
}

impl RetentionStore for RecordingStore {
    fn pin_resolved_range(
        &self,
        request: RetentionPinRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ProgressivePinChange>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.request.session_id)?;
            self.flush_session(request.request.session_id).await?;
            let changed = self.index.pin_resolved_range(&request)?;
            self.enforce_locked().await?;
            Ok(ProgressivePinChange {
                changed,
                state: self.progressive_pin_state_locked(request)?,
            })
        })
    }

    fn unpin_resolved_range(
        &self,
        request: RetentionPinRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ProgressivePinChange>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.request.session_id)?;
            let changed = self.index.unpin_resolved_range(request.request)?;
            self.enforce_locked().await?;
            Ok(ProgressivePinChange {
                changed,
                state: self.progressive_pin_state_locked(request)?,
            })
        })
    }

    fn query_pin_state(
        &self,
        request: RetentionPinRequest,
    ) -> PortFuture<'_, krometrail_core::Result<PinState>> {
        Box::pin(async move {
            let _mutation = self.mutations.lock().await;
            self.reject_deleted(request.request.session_id)?;
            self.progressive_pin_state_locked(request)
        })
    }

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

    /// Reports retention status without taking the mutation gate.
    ///
    /// Status is a read. Serialising it behind the gate made it wait out whatever
    /// eviction or deletion happened to be running, which is the opposite of what
    /// an agent checking on a store under pressure needs. It therefore also skips
    /// the WAL checkpoint that `refresh_usage` performs: object usage comes
    /// straight from the accounting rows and is exact, while the index-page class
    /// may lag until the next mutation refreshes it. Trading a slightly stale
    /// page figure for a status call that never blocks is the right side of that
    /// bargain, and every mutating path still refreshes before it decides
    /// anything.
    fn status(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        Box::pin(async move {
            self.status_from_snapshot(
                self.index.live_usage_snapshot()?,
                self.current_budget_state(),
            )
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

fn source_read_not_found(snapshot: Option<&FrameReadSnapshot>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("source evidence changed or was evicted during the read")
            .expect("static source read error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: snapshot.map(|value| value.metadata.session_id()),
        target_id: snapshot.map(|value| value.metadata.target_id()),
        ..Default::default()
    })
    .with_recovery(
        NonEmptyText::new("resolve and list the temporal range again")
            .expect("static source read recovery is non-empty"),
    )
}

fn source_read_not_found_for_scope(scope: EvidenceScope) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("source frame is not retained in the requested evidence scope")
            .expect("static scoped source read error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(scope.session_id),
        target_id: Some(scope.target_id),
        ..Default::default()
    })
    .with_recovery(
        NonEmptyText::new("resolve and list the temporal range again")
            .expect("static scoped source read recovery is non-empty"),
    )
}

fn source_limit_error(message: &'static str, scope: EvidenceScope) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("static source limit error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(scope.session_id),
        target_id: Some(scope.target_id),
        ..Default::default()
    })
    .with_recovery(
        NonEmptyText::new("select fewer source frames or lower encoded-byte limits")
            .expect("static source limit recovery is non-empty"),
    )
}

fn artifact_limit_error(scope: EvidenceScope) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new("artifact exceeds the requested encoded-byte limit")
            .expect("static artifact limit error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(scope.session_id),
        target_id: Some(scope.target_id),
        ..Default::default()
    })
}

fn artifact_not_found(session_id: SessionId, target_id: TargetId) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("artifact or its retained sources changed during the read")
            .expect("static artifact read error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        ..Default::default()
    })
}

fn evidence_invalidated_error(session_id: SessionId, target_id: TargetId) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::EvidenceInvalidated,
        NonEmptyText::new("derived artifact failed exact provenance or payload validation")
            .expect("static artifact invalidation error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        ..Default::default()
    })
    .with_recovery(
        NonEmptyText::new("regenerate the artifact if its source frames remain retained")
            .expect("static artifact invalidation recovery is non-empty"),
    )
}

fn artifact_object(candidate: ArtifactCandidate) -> DeletionObject {
    DeletionObject {
        kind: DeletionObjectKind::Artifact(candidate.artifact_id),
        relative_path: candidate.relative_path,
        byte_len: candidate.byte_len,
        session_id: candidate.session_id,
    }
}

/// Builds the deletion object for an evictable segment.
///
/// The file name is derived as the *sealed* name for this segment id, never read
/// back from storage. Both candidate queries that feed this already restrict to
/// `state='sealed'`, so deriving here means a deletion object naming a live
/// `.open` file cannot be constructed even if a future query forgets the filter.
fn segment_object(candidate: SegmentCandidate) -> DeletionObject {
    DeletionObject {
        kind: DeletionObjectKind::Segment(candidate.segment_id),
        relative_path: crate::segments::segment_file_name(
            candidate.segment_id,
            crate::SegmentState::Sealed,
        ),
        byte_len: candidate.file_bytes,
        session_id: candidate.session_id,
    }
}

fn context_query_not_found(range: &ResolvedRange) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("temporal context source range is no longer retained")
            .expect("static context error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(range.session_id),
        target_id: Some(range.target_id),
        range: Some(range.resolved_range),
        ..ErrorContext::default()
    })
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("resolve the temporal range again before requesting context")
            .expect("static context recovery is non-empty"),
    )
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

    fn recording_test_clock() -> Arc<dyn MonotonicClock> {
        struct Fixed;
        impl MonotonicClock for Fixed {
            fn now(&self) -> ObservedTime {
                ObservedTime::from_nanos(0)
            }
        }
        Arc::new(Fixed)
    }
    use std::{sync::Arc, time::Duration};

    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, EvidenceScope, FrameId,
        FrameSource, ImageFormat, ObservedTime, PixelDimensions, RecordingSink,
        RetrieveSourceFrameRequest, SessionTime, TargetId,
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
        let store = RecordingStore::new(
            Arc::clone(&writer),
            Arc::clone(&index),
            recording_test_clock(),
        )
        .unwrap();
        (index, writer, store)
    }

    fn artifact_publication(source: &EncodedFrame) -> ArtifactPublication {
        use std::num::{NonZeroU32, NonZeroUsize};
        use temporal_vision::{
            DeclaredGap, FilmstripTileLimit, Frame, FrameSequence, IntegerScale, Marker,
            PixelDimensions as VisionDimensions, PixelFormat, RegionDefinition,
            RegionFilmstripLabels, RegionFilmstripParameters, RegionFilmstripRenderLimits, Rgb8,
            SignedPixelRect, Timestamp, generate_region_filmstrip, generator_descriptor,
        };

        let dimensions = VisionDimensions::new(1, 1).unwrap();
        let sequence = FrameSequence::new(
            vec![
                Frame::new(
                    source.metadata().id(),
                    Timestamp::from_nanos(source.metadata().session_time().as_nanos()),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    vec![1, 2, 3, 255].into_boxed_slice(),
                )
                .unwrap(),
            ],
            Vec::<Marker<krometrail_core::ArtifactMarkerId>>::new(),
            Vec::<DeclaredGap<krometrail_core::GapId>>::new(),
            None,
            None,
        )
        .unwrap();
        let artifact_id = krometrail_core::ArtifactId::from_uuid(Uuid::from_u128(900));
        let generated = generate_region_filmstrip(
            artifact_id,
            &sequence,
            RegionFilmstripParameters::new(
                RegionDefinition::FixedSourceImage {
                    rect: SignedPixelRect::new(
                        0,
                        0,
                        NonZeroU32::new(1).unwrap(),
                        NonZeroU32::new(1).unwrap(),
                    )
                    .unwrap(),
                },
                Timestamp::from_nanos(source.metadata().session_time().as_nanos()),
                FilmstripTileLimit::new(1).unwrap(),
                Rgb8::new(0, 0, 0),
                Rgb8::new(1, 1, 1),
                IntegerScale::IDENTITY,
                RegionFilmstripLabels::new("region", "fixture").unwrap(),
                RegionFilmstripRenderLimits::new(
                    NonZeroU32::new(1024).unwrap(),
                    NonZeroU32::new(1024).unwrap(),
                    NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
                    NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
                ),
            ),
        )
        .unwrap();
        let descriptor = generator_descriptor(temporal_vision::ArtifactKind::RegionFilmstrip);
        ArtifactPublication::new(
            source.metadata().session_id(),
            source.metadata().target_id(),
            vec![ArtifactSourceFingerprint {
                frame_id: source.metadata().id(),
                encoded_sha256: Sha256::digest(source.bytes()).into(),
            }],
            krometrail_core::ArtifactCacheMetadata {
                cache_key: ArtifactCacheKey::from_bytes([7; 32]),
                source_fingerprint: [8; 32],
                parameter_hash: [9; 32],
                visual_epoch_hash: [10; 32],
                cache_schema_version: 1,
                adapter_version: NonEmptyText::new("adapter-v1").unwrap(),
                generator_name: NonEmptyText::new(descriptor.name).unwrap(),
                generator_version: NonEmptyText::new(descriptor.version).unwrap(),
            },
            generated.manifest().clone(),
            NonEmptyText::new("image/png").unwrap(),
            generated.image().bytes().to_vec(),
        )
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn coherent_artifact_read_rejects_concurrent_session_deletion() {
        let directory = TempDir::new().unwrap();
        let (_index, _writer, store) = fixture(&directory);
        let store = Arc::new(store);
        let source = frame(80, 81, 82, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();
        let publication = artifact_publication(&source);
        store.publish_artifact(publication.clone()).await.unwrap();

        let pause = store.pause_next_evidence_read(EvidenceReadKind::Artifact);
        let reached = pause.reached.notified();
        tokio::pin!(reached);
        reached.as_mut().enable();
        let reader = {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                store
                    .read_artifact(
                        RetrieveArtifactRequest::new(
                            EvidenceScope::new(
                                source.metadata().session_id(),
                                source.metadata().target_id(),
                            )
                            .unwrap(),
                            *publication.manifest.artifact_id(),
                            publication.encoded_bytes.len() as u64,
                        )
                        .unwrap(),
                    )
                    .await
            })
        };
        reached.await;
        store
            .delete_session(SessionId::from_uuid(Uuid::from_u128(80)))
            .await
            .unwrap();
        pause.resume.notify_one();
        assert_eq!(reader.await.unwrap().unwrap(), ArtifactReadLookup::Missing);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn coherent_artifact_read_rejects_source_link_hash_change() {
        let directory = TempDir::new().unwrap();
        let (index, _writer, store) = fixture(&directory);
        let store = Arc::new(store);
        let source = frame(85, 86, 87, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();
        let publication = artifact_publication(&source);
        let artifact_id = *publication.manifest.artifact_id();
        let encoded_len = publication.encoded_bytes.len() as u64;
        store.publish_artifact(publication).await.unwrap();

        let pause = store.pause_next_evidence_read(EvidenceReadKind::Artifact);
        let reached = pause.reached.notified();
        tokio::pin!(reached);
        reached.as_mut().enable();
        let reader = {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                store
                    .read_artifact(
                        RetrieveArtifactRequest::new(
                            EvidenceScope::new(
                                source.metadata().session_id(),
                                source.metadata().target_id(),
                            )
                            .unwrap(),
                            artifact_id,
                            encoded_len,
                        )
                        .unwrap(),
                    )
                    .await
            })
        };
        reached.await;
        index
            .connection()
            .unwrap()
            .execute(
                "UPDATE artifact_frames SET encoded_hash=?1 WHERE artifact_id=?2",
                rusqlite::params![
                    vec![0_u8; 32],
                    crate::index::codec::id(artifact_id.as_uuid()).to_vec(),
                ],
            )
            .unwrap();
        pause.resume.notify_one();
        assert_eq!(reader.await.unwrap().unwrap(), ArtifactReadLookup::Missing);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn coherent_source_read_releases_gate_and_rejects_concurrent_deletion() {
        let directory = TempDir::new().unwrap();
        let (_index, _writer, store) = fixture(&directory);
        let store = Arc::new(store);
        let source = frame(40, 41, 42, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();

        let pause = store.pause_next_evidence_read(EvidenceReadKind::Source);
        let reached = pause.reached.notified();
        tokio::pin!(reached);
        reached.as_mut().enable();
        let reader = {
            let store = Arc::clone(&store);
            tokio::spawn(async move { store.frames_by_id(vec![source.metadata().id()]).await })
        };
        reached.await;

        // Both operations require the mutation gate. Their completion while the
        // read is paused proves file I/O does not hold it.
        let unrelated = frame(50, 51, 52, 1);
        store.append_frame(unrelated).await.unwrap();
        store
            .delete_session(SessionId::from_uuid(Uuid::from_u128(40)))
            .await
            .unwrap();
        pause.resume.notify_one();
        assert_eq!(reader.await.unwrap().unwrap_err().code, ErrorCode::NotFound);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn scoped_source_resource_read_returns_only_a_revalidated_snapshot() {
        let directory = TempDir::new().unwrap();
        let (_index, _writer, store) = fixture(&directory);
        let source = frame(45, 46, 47, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();
        let scope = EvidenceScope::new(
            source.metadata().session_id(),
            source.metadata().target_id(),
        )
        .unwrap();
        let read = store
            .read_source_frame(
                RetrieveSourceFrameRequest::new(scope, source.metadata().id(), 1024).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read.encoded_bytes(), source.bytes());
        assert_eq!(read.handle.scope, scope);
        assert_eq!(read.handle.frame_id, source.metadata().id());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn scoped_source_resource_read_rejects_concurrent_deletion_without_bytes() {
        let directory = TempDir::new().unwrap();
        let (_index, _writer, store) = fixture(&directory);
        let store = Arc::new(store);
        let source = frame(48, 49, 50, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();
        let scope = EvidenceScope::new(
            source.metadata().session_id(),
            source.metadata().target_id(),
        )
        .unwrap();
        let session_id = source.metadata().session_id();
        let frame_id = source.metadata().id();

        let pause = store.pause_next_evidence_read(EvidenceReadKind::Source);
        let reached = pause.reached.notified();
        tokio::pin!(reached);
        reached.as_mut().enable();
        let reader = {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                store
                    .read_source_frame(
                        RetrieveSourceFrameRequest::new(scope, frame_id, 1024).unwrap(),
                    )
                    .await
            })
        };
        reached.await;
        store.delete_session(session_id).await.unwrap();
        pause.resume.notify_one();
        let error = reader.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn coherent_source_read_rejects_concurrent_eviction() {
        let directory = TempDir::new().unwrap();
        let (index, _writer, store) = fixture(&directory);
        let store = Arc::new(store);
        let source = frame(55, 56, 57, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();

        let pause = store.pause_next_evidence_read(EvidenceReadKind::Source);
        let reached = pause.reached.notified();
        tokio::pin!(reached);
        reached.as_mut().enable();
        let reader = {
            let store = Arc::clone(&store);
            tokio::spawn(async move { store.frames_by_id(vec![source.metadata().id()]).await })
        };
        reached.await;
        let candidate = index.oldest_unpinned_segment().unwrap().unwrap();
        store
            .remove_objects(
                DeletionKind::Eviction,
                None,
                vec![segment_object(candidate)],
            )
            .await
            .unwrap();
        pause.resume.notify_one();
        assert_eq!(reader.await.unwrap().unwrap_err().code, ErrorCode::NotFound);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn coherent_source_read_rejects_metadata_change_after_snapshot() {
        let directory = TempDir::new().unwrap();
        let (index, _writer, store) = fixture(&directory);
        let store = Arc::new(store);
        let source = frame(60, 61, 62, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();

        let pause = store.pause_next_evidence_read(EvidenceReadKind::Source);
        let reached = pause.reached.notified();
        tokio::pin!(reached);
        reached.as_mut().enable();
        let reader = {
            let store = Arc::clone(&store);
            tokio::spawn(async move { store.frames_by_id(vec![source.metadata().id()]).await })
        };
        reached.await;
        index
            .connection()
            .unwrap()
            .execute(
                "UPDATE frames SET capture_ordinal_be=?1 WHERE frame_id=?2",
                rusqlite::params![
                    crate::index::codec::u64_blob(2).to_vec(),
                    crate::index::codec::id(FrameId::from_uuid(Uuid::from_u128(62)).as_uuid())
                        .to_vec(),
                ],
            )
            .unwrap();
        pause.resume.notify_one();
        assert_eq!(reader.await.unwrap().unwrap_err().code, ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn source_corruption_is_never_reported_as_eviction() {
        let directory = TempDir::new().unwrap();
        let (index, _writer, store) = fixture(&directory);
        let source = frame(70, 71, 72, 1);
        store.append_frame(source.clone()).await.unwrap();
        store.flush(source.metadata().session_id()).await.unwrap();
        let candidate = index.oldest_unpinned_segment().unwrap().unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(crate::segments::sealed_segment_path(
                index.segments_directory(),
                candidate.segment_id,
            ))
            .unwrap()
            .set_len(8)
            .unwrap();
        assert_eq!(
            store
                .frames_by_id(vec![source.metadata().id()])
                .await
                .unwrap_err()
                .code,
            ErrorCode::PersistenceFailed
        );
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

            let reopened = RecordingStore::new(
                Arc::clone(&writer),
                Arc::clone(&index),
                recording_test_clock(),
            )
            .unwrap();
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
