use std::sync::Arc;

use crate::{
    CaptureOrdinal, CapturedFrame, EncodedFrame, ErrorCode, FrameAvailability, FrameId,
    KrometrailError, NonEmptyText, Result, SessionId, SessionRange, SourceFrameBatch,
    SourceFrameList, SourceFramesRequest, TargetId,
};

use super::PortFuture;

/// Reads retained encoded frames without exposing their physical storage.
pub trait FrameSource: Send + Sync {
    /// Lists bounded source handles after hashing exact encoded payloads.
    ///
    /// The default keeps existing non-progressive adapters source-compatible;
    /// production progressive authorities override it with coherent reads.
    fn list_source_frames(
        &self,
        _request: SourceFramesRequest,
    ) -> PortFuture<'_, Result<SourceFrameList>> {
        Box::pin(std::future::ready(Err(progressive_read_unsupported())))
    }

    /// Fetches bounded source handles and request-scoped encoded payloads.
    fn fetch_source_frames(
        &self,
        _request: SourceFramesRequest,
    ) -> PortFuture<'_, Result<SourceFrameBatch>> {
        Box::pin(std::future::ready(Err(progressive_read_unsupported())))
    }

    /// Returns exactly one frame per id in request order; any missing id fails the request.
    fn frames_by_id(&self, frame_ids: Vec<FrameId>) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;

    /// Returns metadata for exactly one retained frame per requested id, in request order.
    fn frame_metadata_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, Result<Vec<CapturedFrame>>>;

    /// Returns retained frames for one target in capture-ordinal order.
    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;

    /// Returns retained frames inclusively between per-target capture ordinals.
    fn frames_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;

    /// Returns retained metadata in capture-ordinal order without reading segment payloads.
    fn frame_metadata_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<CapturedFrame>>>;

    /// Returns retained metadata inclusively between capture ordinals.
    fn frame_metadata_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, Result<Vec<CapturedFrame>>>;

    /// Reports retained bounds separately from durable eviction truth.
    fn frame_availability(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<FrameAvailability>>;
}

impl<T: FrameSource + ?Sized> FrameSource for Arc<T> {
    fn list_source_frames(
        &self,
        request: SourceFramesRequest,
    ) -> PortFuture<'_, Result<SourceFrameList>> {
        (**self).list_source_frames(request)
    }
    fn fetch_source_frames(
        &self,
        request: SourceFramesRequest,
    ) -> PortFuture<'_, Result<SourceFrameBatch>> {
        (**self).fetch_source_frames(request)
    }
    fn frames_by_id(&self, frame_ids: Vec<FrameId>) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
        (**self).frames_by_id(frame_ids)
    }
    fn frame_metadata_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
        (**self).frame_metadata_by_id(frame_ids)
    }
    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
        (**self).frames_in_range(session_id, target_id, range)
    }
    fn frames_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
        (**self).frames_in_ordinal_range(session_id, target_id, start, end)
    }
    fn frame_metadata_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
        (**self).frame_metadata_in_range(session_id, target_id, range)
    }
    fn frame_metadata_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
        (**self).frame_metadata_in_ordinal_range(session_id, target_id, start, end)
    }
    fn frame_availability(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<FrameAvailability>> {
        (**self).frame_availability(session_id, target_id)
    }
}

fn progressive_read_unsupported() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Unsupported,
        NonEmptyText::new("this frame source does not provide coherent progressive reads")
            .expect("static progressive frame error is non-empty"),
    )
}
