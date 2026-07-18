use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use krometrail_core::{
    ErrorCode, ErrorContext, FrameSource, IdSource, KrometrailError, MAX_RESOLVED_RANGE_HANDLES,
    NonEmptyText, ResolvedRange, ResolvedRangeHandleId, ResolvedRangeHandles, Result, RetryAdvice,
    SessionId,
};

pub(crate) struct ProcessResolvedRangeHandles {
    ids: Arc<dyn IdSource>,
    frames: Arc<dyn FrameSource>,
    entries: Mutex<HashMap<ResolvedRangeHandleId, ResolvedRange>>,
}

impl ProcessResolvedRangeHandles {
    pub(crate) fn new(ids: Arc<dyn IdSource>, frames: Arc<dyn FrameSource>) -> Self {
        Self {
            ids,
            frames,
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl ResolvedRangeHandles for ProcessResolvedRangeHandles {
    fn register(&self, range: ResolvedRange) -> Result<ResolvedRangeHandleId> {
        range.validate()?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| internal_authority_error())?;
        if let Some((handle, _)) = entries.iter().find(|(_, existing)| **existing == range) {
            return Ok(*handle);
        }
        if entries.len() >= MAX_RESOLVED_RANGE_HANDLES {
            return Err(KrometrailError::new(
                ErrorCode::ResourceLimitExceeded,
                NonEmptyText::new("resolved range handle capacity has been reached")
                    .expect("static handle limit message is non-empty"),
            ));
        }
        let handle = ResolvedRangeHandleId::from_uuid(*self.ids.next().as_uuid());
        if handle.as_uuid().is_nil() || entries.contains_key(&handle) {
            return Err(internal_authority_error());
        }
        entries.insert(handle, range);
        Ok(handle)
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
                .get(&handle)
                .cloned()
                .ok_or_else(|| invalidated_handle(None))?;
            let metadata = self
                .frames
                .frame_metadata_by_id(range.frame_ids.clone())
                .await
                .map_err(|_| invalidated_handle(Some(&range)))?;
            let valid = metadata.len() == range.frame_ids.len()
                && metadata
                    .iter()
                    .zip(&range.frame_ids)
                    .all(|(frame, expected_id)| {
                        frame.id() == *expected_id
                            && frame.session_id() == range.session_id
                            && frame.target_id() == range.target_id
                            && range.resolved_range.contains(frame.session_time())
                    });
            if !valid {
                return Err(invalidated_handle(Some(&range)));
            }
            Ok(range)
        })
    }

    fn invalidate_session(&self, session_id: SessionId) -> Result<usize> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| internal_authority_error())?;
        let before = entries.len();
        entries.retain(|_, range| range.session_id != session_id);
        Ok(before - entries.len())
    }
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
        RetrieveSourceFrameRequest, SessionRange, SessionTime, SourceFrameBatch, SourceFrameList,
        SourceFrameRead, SourceFramesRequest, SourceTime, TargetId, TemporalRangeAnchorKind,
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

    struct MutableFrames(Mutex<Vec<CapturedFrame>>);

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
            let frames = self.0.lock().unwrap().clone();
            Box::pin(std::future::ready(Ok(frames)))
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
        let frames = Arc::new(MutableFrames(Mutex::new(vec![frame(
            range.frame_ids[0],
            range.session_id,
            range.target_id,
            5,
        )])));
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
        let handle = authority.register(range.clone()).unwrap();
        assert_eq!(authority.register(range.clone()).unwrap(), handle);
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
        let frames = Arc::new(MutableFrames(Mutex::new(vec![
            frame(range.frame_ids[0], range.session_id, range.target_id, 3),
            frame(range.frame_ids[1], range.session_id, range.target_id, 7),
        ])));
        let authority = ProcessResolvedRangeHandles::new(
            Arc::new(CountingIds(AtomicU64::new(200))),
            Arc::clone(&frames) as Arc<dyn FrameSource>,
        );
        let handle = authority.register(range).unwrap();
        frames.0.lock().unwrap().reverse();
        assert_eq!(
            authority.resolve_available(handle).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
    }

    #[tokio::test]
    async fn unknown_partial_cross_scope_and_invalidated_session_fail() {
        let range = range(20);
        let (authority, frames) = authority(&range);
        let unknown = ResolvedRangeHandleId::from_uuid(uuid::Uuid::from_u128(999));
        assert_eq!(
            authority.resolve_available(unknown).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
        let handle = authority.register(range.clone()).unwrap();
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
        assert_eq!(authority.invalidate_session(range.session_id).unwrap(), 1);
        assert_eq!(
            authority.resolve_available(handle).await.unwrap_err().code,
            ErrorCode::EvidenceInvalidated
        );
    }

    #[test]
    fn capacity_and_collisions_never_evict_existing_handles() {
        let seed = range(30);
        let (authority, _) = authority(&seed);
        let first = authority.register(seed).unwrap();
        for value in 31..(31 + MAX_RESOLVED_RANGE_HANDLES as u128 - 1) {
            authority.register(range(value)).unwrap();
        }
        assert_eq!(
            authority.register(range(50_000)).unwrap_err().code,
            ErrorCode::ResourceLimitExceeded
        );
        assert!(authority.entries.lock().unwrap().contains_key(&first));

        let frames: Arc<dyn FrameSource> = Arc::new(MutableFrames(Mutex::new(Vec::new())));
        let colliding = ProcessResolvedRangeHandles::new(Arc::new(ConstantIds), frames);
        colliding.register(range(60_000)).unwrap();
        assert_eq!(
            colliding.register(range(60_001)).unwrap_err().code,
            ErrorCode::Internal
        );
    }
}
