use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use krometrail_core::{
    CapturedFrame, ErrorCode, ErrorContext, FrameSource, IdSource, KrometrailError,
    MAX_RESOLVED_RANGE_HANDLE_BUDGET_BYTES, MAX_RESOLVED_RANGE_HANDLES, NonEmptyText,
    ResolvedRange, ResolvedRangeHandleId, ResolvedRangeHandles, Result, RetryAdvice,
};

pub(crate) struct ProcessResolvedRangeHandles {
    ids: Arc<dyn IdSource>,
    frames: Arc<dyn FrameSource>,
    entries: Mutex<RangeHandleEntries>,
    max_entries: usize,
    max_budget_bytes: usize,
}

#[derive(Default)]
struct RangeHandleEntries {
    ranges: HashMap<ResolvedRangeHandleId, ResolvedRange>,
    used_budget_bytes: usize,
}

impl ProcessResolvedRangeHandles {
    pub(crate) fn new(ids: Arc<dyn IdSource>, frames: Arc<dyn FrameSource>) -> Self {
        Self {
            ids,
            frames,
            entries: Mutex::new(RangeHandleEntries::default()),
            max_entries: MAX_RESOLVED_RANGE_HANDLES,
            max_budget_bytes: MAX_RESOLVED_RANGE_HANDLE_BUDGET_BYTES,
        }
    }

    #[cfg(test)]
    fn with_limits(
        ids: Arc<dyn IdSource>,
        frames: Arc<dyn FrameSource>,
        max_entries: usize,
        max_budget_bytes: usize,
    ) -> Self {
        Self {
            ids,
            frames,
            entries: Mutex::new(RangeHandleEntries::default()),
            max_entries,
            max_budget_bytes,
        }
    }
}

impl ResolvedRangeHandles for ProcessResolvedRangeHandles {
    fn register(
        &self,
        range: ResolvedRange,
    ) -> krometrail_core::PortFuture<'_, Result<ResolvedRangeHandleId>> {
        Box::pin(async move {
            range.validate()?;
            let budget_bytes = range_budget_bytes(&range, self.max_budget_bytes)?;
            let metadata = read_available_metadata(self.frames.as_ref(), &range).await?;
            validate_available_metadata(&range, &metadata)?;

            let mut entries = self
                .entries
                .lock()
                .map_err(|_| internal_authority_error())?;
            if let Some((handle, _)) = entries
                .ranges
                .iter()
                .find(|(_, existing)| **existing == range)
            {
                return Ok(*handle);
            }
            let next_budget = entries
                .used_budget_bytes
                .checked_add(budget_bytes)
                .ok_or_else(resource_limit_error)?;
            if entries.ranges.len() >= self.max_entries || next_budget > self.max_budget_bytes {
                return Err(resource_limit_error());
            }
            let handle = ResolvedRangeHandleId::from_uuid(*self.ids.next().as_uuid());
            if handle.as_uuid().is_nil() || entries.ranges.contains_key(&handle) {
                return Err(internal_authority_error());
            }
            entries.ranges.insert(handle, range);
            entries.used_budget_bytes = next_budget;
            Ok(handle)
        })
    }

    fn resolve_available(
        &self,
        handle: ResolvedRangeHandleId,
    ) -> krometrail_core::PortFuture<'_, Result<ResolvedRange>> {
        Box::pin(async move {
            let range = self
                .entries
                .lock()
                .map_err(|_| internal_authority_error())?
                .ranges
                .get(&handle)
                .cloned()
                .ok_or_else(|| invalidated_handle(None))?;
            let metadata = read_available_metadata(self.frames.as_ref(), &range).await?;
            validate_available_metadata(&range, &metadata)?;
            Ok(range)
        })
    }
}

async fn read_available_metadata(
    frames: &dyn FrameSource,
    range: &ResolvedRange,
) -> Result<Vec<CapturedFrame>> {
    frames
        .frame_metadata_by_id(range.frame_ids.clone())
        .await
        .map_err(|error| {
            if error.code == ErrorCode::NotFound {
                invalidated_handle(Some(range))
            } else {
                error
            }
        })
}

fn validate_available_metadata(range: &ResolvedRange, metadata: &[CapturedFrame]) -> Result<()> {
    let exact = metadata.len() == range.frame_ids.len()
        && metadata
            .iter()
            .zip(&range.frame_ids)
            .all(|(frame, expected_id)| {
                frame.id() == *expected_id
                    && frame.session_id() == range.session_id
                    && frame.target_id() == range.target_id
                    && range.resolved_range.contains(frame.session_time())
            });
    let ordered = metadata.windows(2).all(|pair| {
        pair[0].capture_ordinal() < pair[1].capture_ordinal()
            && pair[0].session_time() <= pair[1].session_time()
    });
    if exact && ordered {
        Ok(())
    } else {
        Err(invalidated_handle(Some(range)))
    }
}

fn range_budget_bytes(range: &ResolvedRange, limit: usize) -> Result<usize> {
    let mut writer = BudgetWriter {
        written: 0,
        limit,
        exceeded: false,
    };
    match serde_json::to_writer(&mut writer, range) {
        Ok(()) => Ok(writer.written),
        Err(_) if writer.exceeded => Err(resource_limit_error()),
        Err(_) => Err(internal_authority_error()),
    }
}

struct BudgetWriter {
    written: usize,
    limit: usize,
    exceeded: bool,
}

impl std::io::Write for BudgetWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let Some(next) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(std::io::Error::other("range handle budget exceeded"));
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other("range handle budget exceeded"));
        }
        self.written = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn resource_limit_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new("resolved range handle capacity or memory budget has been reached")
            .expect("static handle limit message is non-empty"),
    )
}

fn invalidated_handle(range: Option<&ResolvedRange>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::EvidenceInvalidated,
        NonEmptyText::new(
            "resolved range handle is unavailable in this process or its retained evidence changed",
        )
        .expect("static invalidation message is non-empty"),
    )
    .with_context(
        range.map_or_else(ErrorContext::default, |range| ErrorContext {
            session_id: Some(range.session_id),
            target_id: Some(range.target_id),
            interaction_id: None,
            range: Some(range.resolved_range),
        }),
    )
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("run temporal_debug_bundle again to resolve currently retained evidence")
            .expect("static invalidation recovery is non-empty"),
    )
}

fn internal_authority_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Internal,
        NonEmptyText::new("resolved range handle authority failed")
            .expect("static authority error is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameAvailability, FrameId,
        IdValue, ImageFormat, ObservedTime, PixelDimensions, PortFuture, RangeResolutionOptions,
        RetrieveSourceFrameRequest, SessionId, SessionRange, SessionTime, SourceFrameBatch,
        SourceFrameList, SourceFrameRead, SourceFramesRequest, SourceTime, TargetId,
        TemporalRangeAnchorKind,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    struct CountingIds(AtomicU64);

    impl IdSource for CountingIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(uuid::Uuid::from_u128(u128::from(
                self.0.fetch_add(1, Ordering::SeqCst),
            )))
        }
    }

    struct ConstantIds;

    impl IdSource for ConstantIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(uuid::Uuid::from_u128(77))
        }
    }

    struct MutableFrames(Mutex<Vec<CapturedFrame>>, Mutex<Option<KrometrailError>>);

    impl MutableFrames {
        fn new(frames: Vec<CapturedFrame>) -> Self {
            Self(Mutex::new(frames), Mutex::new(None))
        }
    }

    impl FrameSource for MutableFrames {
        fn list_source_frames(
            &self,
            _request: SourceFramesRequest,
        ) -> PortFuture<'_, Result<SourceFrameList>> {
            unused()
        }

        fn fetch_source_frames(
            &self,
            _request: SourceFramesRequest,
        ) -> PortFuture<'_, Result<SourceFrameBatch>> {
            unused()
        }

        fn read_source_frame(
            &self,
            _request: RetrieveSourceFrameRequest,
        ) -> PortFuture<'_, Result<SourceFrameRead>> {
            unused()
        }

        fn frames_by_id(
            &self,
            _frame_ids: Vec<FrameId>,
        ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
            unused()
        }

        fn frame_metadata_by_id(
            &self,
            _frame_ids: Vec<FrameId>,
        ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
            let result = self
                .1
                .lock()
                .unwrap()
                .clone()
                .map_or_else(|| Ok(self.0.lock().unwrap().clone()), Err);
            Box::pin(std::future::ready(result))
        }

        fn frames_in_range(
            &self,
            _session_id: SessionId,
            _target_id: TargetId,
            _range: SessionRange,
        ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
            unused()
        }

        fn frames_in_ordinal_range(
            &self,
            _session_id: SessionId,
            _target_id: TargetId,
            _start: CaptureOrdinal,
            _end: CaptureOrdinal,
        ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
            unused()
        }

        fn frame_metadata_in_range(
            &self,
            _session_id: SessionId,
            _target_id: TargetId,
            _range: SessionRange,
        ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
            unused()
        }

        fn frame_metadata_in_ordinal_range(
            &self,
            _session_id: SessionId,
            _target_id: TargetId,
            _start: CaptureOrdinal,
            _end: CaptureOrdinal,
        ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
            unused()
        }

        fn frame_availability(
            &self,
            _session_id: SessionId,
            _target_id: TargetId,
        ) -> PortFuture<'_, Result<FrameAvailability>> {
            unused()
        }
    }

    fn unused<T: Send + 'static>() -> PortFuture<'static, Result<T>> {
        Box::pin(std::future::ready(Err(internal_authority_error())))
    }

    fn session(value: u128) -> SessionId {
        SessionId::from_uuid(uuid::Uuid::from_u128(value))
    }

    fn target(value: u128) -> TargetId {
        TargetId::from_uuid(uuid::Uuid::from_u128(value))
    }

    fn frame_id(value: u128) -> FrameId {
        FrameId::from_uuid(uuid::Uuid::from_u128(value))
    }

    fn frame(id: FrameId, session_id: SessionId, target_id: TargetId, time: u64) -> CapturedFrame {
        CapturedFrame::new(
            id,
            session_id,
            target_id,
            CaptureOrdinal::new(time + 1).unwrap(),
            Some(SourceTime::from_nanos(i128::from(time))),
            ObservedTime::from_nanos(time),
            SessionTime::from_nanos(time),
            ImageFormat::Png,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap()
    }

    fn range(value: u128) -> ResolvedRange {
        let retained = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        ResolvedRange::new(
            session(1),
            target(2),
            TemporalRangeAnchorKind::SessionTime,
            retained,
            retained,
            vec![frame_id(value)],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }

    fn authority(range: &ResolvedRange) -> (ProcessResolvedRangeHandles, Arc<MutableFrames>) {
        let frames = Arc::new(MutableFrames::new(vec![frame(
            range.frame_ids[0],
            range.session_id,
            range.target_id,
            5,
        )]));
        let authority = ProcessResolvedRangeHandles::new(
            Arc::new(CountingIds(AtomicU64::new(100))),
            Arc::clone(&frames) as Arc<dyn FrameSource>,
        );
        (authority, frames)
    }

    #[tokio::test]
    async fn equal_ranges_deduplicate_and_revalidate_exact_order_and_scope() {
        let range = range(10);
        let (authority, frames) = authority(&range);
        let handle = authority.register(range.clone()).await.unwrap();
        assert_eq!(authority.register(range.clone()).await.unwrap(), handle);
        assert_eq!(authority.resolve_available(handle).await.unwrap(), range);

        frames.0.lock().unwrap()[0] = frame(frame_id(11), session(1), target(2), 5);
        let error = authority.resolve_available(handle).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::EvidenceInvalidated);
        assert_eq!(error.retry, RetryAdvice::AfterRecovery);
    }

    #[tokio::test]
    async fn reordered_retained_metadata_is_invalidated() {
        let mut range = range(12);
        range.frame_ids.push(frame_id(13));
        range.validate().unwrap();
        let frames = Arc::new(MutableFrames::new(vec![
            frame(range.frame_ids[0], range.session_id, range.target_id, 3),
            frame(range.frame_ids[1], range.session_id, range.target_id, 7),
        ]));
        let authority = ProcessResolvedRangeHandles::new(
            Arc::new(CountingIds(AtomicU64::new(200))),
            Arc::clone(&frames) as Arc<dyn FrameSource>,
        );
        let handle = authority.register(range).await.unwrap();
        frames.0.lock().unwrap().reverse();
        assert_eq!(
            authority.resolve_available(handle).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
    }

    #[tokio::test]
    async fn reversed_range_is_rejected_at_admission_even_when_ids_match_source_lookup_order() {
        let retained = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        let reversed = ResolvedRange::new(
            session(1),
            target(2),
            TemporalRangeAnchorKind::SessionTime,
            retained,
            retained,
            vec![frame_id(42), frame_id(41)],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap();
        let frames = Arc::new(MutableFrames::new(vec![
            frame(frame_id(42), reversed.session_id, reversed.target_id, 7),
            frame(frame_id(41), reversed.session_id, reversed.target_id, 3),
        ]));
        let authority =
            ProcessResolvedRangeHandles::new(Arc::new(CountingIds(AtomicU64::new(300))), frames);
        assert_eq!(
            authority.register(reversed).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
        assert!(authority.entries.lock().unwrap().ranges.is_empty());
    }

    #[tokio::test]
    async fn source_persistence_failures_are_preserved_instead_of_becoming_invalidation() {
        let resolved = range(50);
        let (authority, frames) = authority(&resolved);
        let handle = authority.register(resolved).await.unwrap();
        *frames.1.lock().unwrap() = Some(KrometrailError::new(
            ErrorCode::PersistenceFailed,
            NonEmptyText::new("fixture recording index failed").unwrap(),
        ));
        let error = authority.resolve_available(handle).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::PersistenceFailed);
        assert_eq!(error.message.as_str(), "fixture recording index failed");

        let admission_error = authority.register(range(51)).await.unwrap_err();
        assert_eq!(admission_error.code, ErrorCode::PersistenceFailed);
        assert_eq!(
            admission_error.message.as_str(),
            "fixture recording index failed"
        );
        assert_eq!(authority.entries.lock().unwrap().ranges.len(), 1);
    }

    #[tokio::test]
    async fn unknown_partial_and_cross_scope_handles_fail() {
        let range = range(20);
        let (authority, frames) = authority(&range);
        let unknown = ResolvedRangeHandleId::from_uuid(uuid::Uuid::from_u128(999));
        assert_eq!(
            authority.resolve_available(unknown).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
        let handle = authority.register(range.clone()).await.unwrap();
        frames.0.lock().unwrap().clear();
        assert_eq!(
            authority.resolve_available(handle).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
        frames
            .0
            .lock()
            .unwrap()
            .push(frame(range.frame_ids[0], session(999), range.target_id, 5));
        assert_eq!(
            authority.resolve_available(handle).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
    }

    #[tokio::test]
    async fn capacity_and_collisions_never_evict_existing_handles() {
        let seed = range(30);
        let (authority, frames) = authority(&seed);
        let first = authority.register(seed).await.unwrap();
        for value in 31..(31 + MAX_RESOLVED_RANGE_HANDLES as u128 - 1) {
            let next = range(value);
            *frames.0.lock().unwrap() =
                vec![frame(next.frame_ids[0], next.session_id, next.target_id, 5)];
            authority.register(next).await.unwrap();
        }
        let overflow = range(50_000);
        *frames.0.lock().unwrap() = vec![frame(
            overflow.frame_ids[0],
            overflow.session_id,
            overflow.target_id,
            5,
        )];
        assert_eq!(
            authority.register(overflow).await.unwrap_err().code,
            ErrorCode::ResourceLimitExceeded
        );
        assert!(
            authority
                .entries
                .lock()
                .unwrap()
                .ranges
                .contains_key(&first)
        );

        let first_range = range(60_000);
        let collision_frames = Arc::new(MutableFrames::new(vec![frame(
            first_range.frame_ids[0],
            first_range.session_id,
            first_range.target_id,
            5,
        )]));
        let colliding = ProcessResolvedRangeHandles::new(
            Arc::new(ConstantIds),
            Arc::clone(&collision_frames) as Arc<dyn FrameSource>,
        );
        colliding.register(first_range).await.unwrap();
        let second_range = range(60_001);
        *collision_frames.0.lock().unwrap() = vec![frame(
            second_range.frame_ids[0],
            second_range.session_id,
            second_range.target_id,
            5,
        )];
        assert_eq!(
            colliding.register(second_range).await.unwrap_err().code,
            ErrorCode::Internal
        );
    }

    #[tokio::test]
    async fn aggregate_budget_rejects_only_the_new_range() {
        let first_range = range(70_000);
        let second_range = range(70_001);
        let first_cost =
            range_budget_bytes(&first_range, MAX_RESOLVED_RANGE_HANDLE_BUDGET_BYTES).unwrap();
        let second_cost =
            range_budget_bytes(&second_range, MAX_RESOLVED_RANGE_HANDLE_BUDGET_BYTES).unwrap();
        let frames = Arc::new(MutableFrames::new(vec![frame(
            first_range.frame_ids[0],
            first_range.session_id,
            first_range.target_id,
            5,
        )]));
        let per_range_limited = ProcessResolvedRangeHandles::with_limits(
            Arc::new(CountingIds(AtomicU64::new(390))),
            Arc::clone(&frames) as Arc<dyn FrameSource>,
            10,
            first_cost - 1,
        );
        assert_eq!(
            per_range_limited
                .register(first_range.clone())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimitExceeded
        );
        assert!(per_range_limited.entries.lock().unwrap().ranges.is_empty());

        let authority = ProcessResolvedRangeHandles::with_limits(
            Arc::new(CountingIds(AtomicU64::new(400))),
            Arc::clone(&frames) as Arc<dyn FrameSource>,
            10,
            first_cost + second_cost - 1,
        );
        let first = authority.register(first_range.clone()).await.unwrap();
        *frames.0.lock().unwrap() = vec![frame(
            second_range.frame_ids[0],
            second_range.session_id,
            second_range.target_id,
            5,
        )];
        assert_eq!(
            authority.register(second_range).await.unwrap_err().code,
            ErrorCode::ResourceLimitExceeded
        );
        {
            let entries = authority.entries.lock().unwrap();
            assert_eq!(entries.ranges.len(), 1);
            assert_eq!(entries.used_budget_bytes, first_cost);
            assert!(entries.ranges.contains_key(&first));
        }
        let entries = authority.entries.lock().unwrap();
        assert_eq!(entries.ranges.len(), 1);
        assert_eq!(entries.used_budget_bytes, first_cost);
        assert!(entries.ranges.contains_key(&first));
    }
}
