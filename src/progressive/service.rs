use std::{sync::Arc, time::Instant};

use krometrail_core::{
    ArtifactGeneration, ArtifactRead, ArtifactReadLookup, ArtifactStore, ErrorCode, ErrorContext,
    FrameSource, KrometrailError, NonEmptyText, PortFuture, ProgressiveEvidence,
    ProgressiveEvidenceContext, ProgressiveEvidenceRequest, ProgressiveEvidenceResult,
    ProgressiveEvidenceStore, Result, RetryAdvice, Sha256Digest, SourceFrameBatch,
    SourceFrameHandle, SourceFrameList, SourceFrameSelection, SourceFramesRequest,
};

use super::region::prepare_region;

#[derive(Clone)]
pub(crate) struct ProgressiveEvidenceService {
    store: Arc<dyn ProgressiveEvidenceStore>,
    artifacts: Arc<dyn ArtifactGeneration>,
}

impl ProgressiveEvidenceService {
    pub(crate) fn new(
        store: Arc<dyn ProgressiveEvidenceStore>,
        artifacts: Arc<dyn ArtifactGeneration>,
    ) -> Self {
        Self { store, artifacts }
    }

    async fn execute_inner(
        &self,
        request: ProgressiveEvidenceRequest,
        context: ProgressiveEvidenceContext,
    ) -> Result<ProgressiveEvidenceResult> {
        check_context(&context)?;
        match request {
            ProgressiveEvidenceRequest::RetrieveArtifact(request) => {
                let request = krometrail_core::RetrieveArtifactRequest::new(
                    request.scope,
                    request.artifact_id,
                    request.max_encoded_bytes(),
                )?;
                let scope = request.scope;
                let artifact_id = request.artifact_id;
                let max_bytes = request.max_encoded_bytes();
                let read = match self.store.read_artifact(request).await? {
                    ArtifactReadLookup::Available(read) => *read,
                    ArtifactReadLookup::Missing => {
                        return Err(artifact_lifetime_error(
                            ErrorCode::NotFound,
                            scope,
                            "artifact evidence is no longer retained",
                        ));
                    }
                    ArtifactReadLookup::Invalidated => {
                        return Err(artifact_lifetime_error(
                            ErrorCode::EvidenceInvalidated,
                            scope,
                            "artifact evidence failed provenance validation",
                        ));
                    }
                };
                check_context(&context)?;
                validate_artifact_read(&read, scope, artifact_id, max_bytes)?;
                Ok(ProgressiveEvidenceResult::RetrieveArtifact(Box::new(read)))
            }
            ProgressiveEvidenceRequest::RetrieveSourceFrame(request) => {
                let request = krometrail_core::RetrieveSourceFrameRequest::new(
                    request.scope,
                    request.frame_id,
                    request.max_encoded_bytes(),
                )?;
                let scope = request.scope;
                let frame_id = request.frame_id;
                let max_bytes = request.max_encoded_bytes();
                let read = self.store.read_source_frame(request).await?;
                check_context(&context)?;
                if read.handle.scope != scope
                    || read.handle.frame_id != frame_id
                    || read.handle.encoded_byte_len > max_bytes
                    || read.encoded_bytes().len() as u64 != read.handle.encoded_byte_len
                    || krometrail_core::Sha256Digest::digest(read.encoded_bytes())
                        != read.handle.content_sha256
                {
                    return Err(KrometrailError::new(
                        ErrorCode::EvidenceInvalidated,
                        NonEmptyText::new("source frame evidence disagrees with its scoped handle")
                            .expect("static source handle error is non-empty"),
                    ));
                }
                Ok(ProgressiveEvidenceResult::RetrieveSourceFrame(Box::new(
                    read,
                )))
            }
            ProgressiveEvidenceRequest::ListSourceFrames(request) => {
                let request = validate_source_request(request)?;
                let result = self.store.list_source_frames(request.clone()).await?;
                check_context(&context)?;
                validate_source_list(&request, &result)?;
                Ok(ProgressiveEvidenceResult::ListSourceFrames(Box::new(
                    result,
                )))
            }
            ProgressiveEvidenceRequest::FetchSourceFrames(request) => {
                let request = validate_source_request(request)?;
                let result = self.store.fetch_source_frames(request.clone()).await?;
                check_context(&context)?;
                validate_source_batch(&request, &result)?;
                Ok(ProgressiveEvidenceResult::FetchSourceFrames(Box::new(
                    result,
                )))
            }
            ProgressiveEvidenceRequest::GenerateArtifacts(request) => {
                let result = self
                    .artifacts
                    .generate(
                        request.into_request(),
                        context.artifact_generation_context(),
                    )
                    .await?;
                Ok(ProgressiveEvidenceResult::GenerateArtifacts(Box::new(
                    result,
                )))
            }
            ProgressiveEvidenceRequest::GenerateRegionFilmstrip(request) => {
                let prepared = prepare_region(self.store.as_ref(), request, &context).await?;
                check_context(&context)?;
                let expected_range = prepared.generation.range().clone();
                let result = self
                    .artifacts
                    .generate(prepared.generation, context.artifact_generation_context())
                    .await?;
                if result.range != expected_range
                    || result.epochs != std::slice::from_ref(&prepared.epoch)
                {
                    return Err(generation_contract_error());
                }
                Ok(ProgressiveEvidenceResult::GenerateRegionFilmstrip(
                    Box::new(krometrail_core::RegionFilmstripEvidence {
                        region: prepared.resolved,
                        generation: result,
                    }),
                ))
            }
            ProgressiveEvidenceRequest::PinResolvedRange(request) => {
                let result = self
                    .store
                    .pin_resolved_range(request.pin_request()?)
                    .await?;
                Ok(ProgressiveEvidenceResult::PinResolvedRange(Box::new(
                    result,
                )))
            }
            ProgressiveEvidenceRequest::UnpinResolvedRange(request) => {
                let result = self
                    .store
                    .unpin_resolved_range(request.pin_request()?)
                    .await?;
                Ok(ProgressiveEvidenceResult::UnpinResolvedRange(Box::new(
                    result,
                )))
            }
            ProgressiveEvidenceRequest::QueryPinState(request) => {
                let result = self.store.query_pin_state(request.pin_request()?).await?;
                Ok(ProgressiveEvidenceResult::QueryPinState(Box::new(result)))
            }
        }
    }
}

impl ProgressiveEvidence for ProgressiveEvidenceService {
    fn execute(
        &self,
        request: ProgressiveEvidenceRequest,
        context: ProgressiveEvidenceContext,
    ) -> PortFuture<'_, Result<ProgressiveEvidenceResult>> {
        let service = self.clone();
        Box::pin(async move { service.execute_inner(request, context).await })
    }
}

fn validate_source_request(request: SourceFramesRequest) -> Result<SourceFramesRequest> {
    SourceFramesRequest::new(request.range, request.selection, request.limits)
}

fn validate_source_list(request: &SourceFramesRequest, result: &SourceFrameList) -> Result<()> {
    validate_source_handles(request, &result.range, result.frames.iter())
}

fn validate_source_batch(request: &SourceFramesRequest, result: &SourceFrameBatch) -> Result<()> {
    validate_source_handles(
        request,
        &result.range,
        result.frames.iter().map(|read| &read.handle),
    )?;
    for read in &result.frames {
        if read.encoded_bytes().len() as u64 != read.handle.encoded_byte_len
            || Sha256Digest::digest(read.encoded_bytes()) != read.handle.content_sha256
        {
            return Err(source_contract_error(
                request,
                "source payload disagrees with its exact handle",
            ));
        }
    }
    Ok(())
}

fn validate_source_handles<'a>(
    request: &SourceFramesRequest,
    result_range: &krometrail_core::ResolvedRange,
    handles: impl IntoIterator<Item = &'a SourceFrameHandle>,
) -> Result<()> {
    if result_range != &request.range {
        return Err(source_contract_error(
            request,
            "source result changed the resolved range",
        ));
    }
    let selected = match &request.selection {
        SourceFrameSelection::ResolvedOrder => request.range.frame_ids.as_slice(),
        SourceFrameSelection::Ids(ids) => ids.as_slice(),
    };
    let handles = handles.into_iter().collect::<Vec<_>>();
    if handles.len() != selected.len() {
        return Err(source_contract_error(
            request,
            "source result did not return the exact selected frame set",
        ));
    }
    let mut total = 0_u64;
    for (request_position, (expected_id, handle)) in selected.iter().zip(&handles).enumerate() {
        let resolved_position = request
            .range
            .frame_ids
            .iter()
            .position(|id| id == expected_id)
            .expect("validated selection is a subset of the resolved range");
        let expected_media = match handle.provenance.format() {
            krometrail_core::ImageFormat::Jpeg => "image/jpeg",
            krometrail_core::ImageFormat::Png => "image/png",
        };
        if handle.frame_id != *expected_id
            || handle.scope.session_id != request.range.session_id
            || handle.scope.target_id != request.range.target_id
            || handle.request_position != request_position as u32
            || handle.resolved_position != resolved_position as u32
            || handle.provenance.id() != *expected_id
            || handle.provenance.session_id() != request.range.session_id
            || handle.provenance.target_id() != request.range.target_id
            || !request
                .range
                .resolved_range
                .contains(handle.provenance.session_time())
            || handle.media_type.as_str() != expected_media
            || handle.encoded_byte_len == 0
        {
            return Err(source_contract_error(
                request,
                "source handle scope, position, metadata, media type, or length is inconsistent",
            ));
        }
        if handle.encoded_byte_len > request.limits.max_item_bytes() {
            return Err(source_limit_error(
                request,
                "source result exceeds the per-item encoded-byte limit",
            ));
        }
        total = total.checked_add(handle.encoded_byte_len).ok_or_else(|| {
            source_limit_error(request, "source result encoded-byte total overflow")
        })?;
        if total > request.limits.max_total_bytes() {
            return Err(source_limit_error(
                request,
                "source result exceeds the total encoded-byte limit",
            ));
        }
    }
    if matches!(request.selection, SourceFrameSelection::ResolvedOrder)
        && handles.windows(2).any(|pair| {
            pair[0].provenance.capture_ordinal() >= pair[1].provenance.capture_ordinal()
        })
    {
        return Err(source_contract_error(
            request,
            "all-frame source result is not in strict capture order",
        ));
    }
    Ok(())
}

fn validate_artifact_read(
    read: &ArtifactRead,
    scope: krometrail_core::EvidenceScope,
    artifact_id: krometrail_core::ArtifactId,
    max_bytes: u64,
) -> Result<()> {
    if read.handle.scope != scope
        || read.handle.artifact_id != artifact_id
        || read.handle.encoded_byte_len == 0
        || read.encoded_bytes().len() as u64 != read.handle.encoded_byte_len
        || Sha256Digest::digest(read.encoded_bytes()) != read.handle.content_sha256
    {
        return Err(artifact_lifetime_error(
            ErrorCode::EvidenceInvalidated,
            scope,
            "artifact evidence disagrees with its scoped handle",
        ));
    }
    if read.handle.encoded_byte_len > max_bytes {
        return Err(artifact_lifetime_error(
            ErrorCode::ResourceLimitExceeded,
            scope,
            "artifact evidence exceeds the encoded-byte limit",
        ));
    }
    Ok(())
}

fn check_context(context: &ProgressiveEvidenceContext) -> Result<()> {
    if context.is_cancelled() {
        return Err(KrometrailError::new(
            ErrorCode::Cancelled,
            NonEmptyText::new("progressive evidence request was cancelled")
                .expect("static cancellation error is non-empty"),
        ));
    }
    if context
        .deadline
        .is_some_and(|deadline| deadline <= Instant::now())
    {
        return Err(KrometrailError::new(
            ErrorCode::Cancelled,
            NonEmptyText::new("progressive evidence request deadline elapsed")
                .expect("static deadline error is non-empty"),
        ));
    }
    Ok(())
}

fn artifact_lifetime_error(
    code: ErrorCode,
    scope: krometrail_core::EvidenceScope,
    message: &'static str,
) -> KrometrailError {
    let mut error = KrometrailError::new(
        code,
        NonEmptyText::new(message).expect("artifact lifetime errors are non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(scope.session_id),
        target_id: Some(scope.target_id),
        interaction_id: None,
        range: None,
    })
    .with_retry(code.default_retry());
    if let Some(recovery) = code.default_recovery() {
        error = error.with_recovery(
            NonEmptyText::new(recovery).expect("default recovery guidance is non-empty"),
        );
    }
    error
}

fn source_contract_error(request: &SourceFramesRequest, message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new(message).expect("source contract errors are non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(request.range.session_id),
        target_id: Some(request.range.target_id),
        interaction_id: None,
        range: Some(request.range.resolved_range),
    })
    .with_retry(RetryAdvice::Never)
}

fn source_limit_error(request: &SourceFramesRequest, message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("source limit errors are non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(request.range.session_id),
        target_id: Some(request.range.target_id),
        interaction_id: None,
        range: Some(request.range.resolved_range),
    })
    .with_recovery(
        NonEmptyText::new("request fewer source frames or lower encoded-byte limits")
            .expect("source limit recovery is non-empty"),
    )
}

fn generation_contract_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new("region artifact generation contradicted its fixed one-epoch request")
            .expect("generation contract error is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        AnalysisScale, ArtifactCacheKey, ArtifactEvidenceHandle, ArtifactFailurePolicy,
        ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGenerationResult,
        ArtifactGeneratorRequest, ArtifactLabelsRequest, ArtifactLookup, ArtifactPublication,
        ArtifactPublish, ArtifactSourceFingerprint, CaptureOrdinal, CapturedFrame,
        CurrentReferenceGeometry, CurrentReferenceGeometryRequest, DeviceScaleFactor,
        DifferenceMapRequest, DiskBudgetBytes, EncodedFrame, EvidenceScope, FrameAvailability,
        FrameId, FrameSelector, GenerateArtifactsRequest, ImageFormat, MotionHistoryRequest,
        NormalizationRequest, ObservedTime, OutputLimitsRequest, PinProtectionScope, PinState,
        ProgressivePinChange, RangeEvidenceAvailability, RangeResolutionOptions,
        RecordingBudgetState, RegionFilmstripEvidenceRequest, RegionFilmstripRequest,
        ResolvedRange, ResolvedRangeEvidenceRequest, ResolvedReferenceGeometry,
        RetentionPinRequest, RetentionRange, RetentionStatus, SessionDeletion, SessionId,
        SessionRange, SessionTime, SnapshotGeneration, SnapshotNodeId, SourceFrameRead,
        SourceReadLimitsRequest, StorageUsage, StoryboardRequest, TargetId,
        TemporalRangeAnchorKind,
    };
    use std::{
        num::NonZeroU32,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use temporal_vision::{
        ArtifactKind, BinaryMask, FrequencyMode, RegionDefinition, Rgb8, SignedPixelRect,
    };
    use uuid::Uuid;

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }
    fn frame_id(value: u128) -> FrameId {
        FrameId::from_uuid(Uuid::from_u128(value))
    }
    fn range() -> ResolvedRange {
        let time =
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(2)).unwrap();
        ResolvedRange::new(
            session(),
            target(),
            TemporalRangeAnchorKind::SessionTime,
            time,
            time,
            vec![frame_id(3), frame_id(4)],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }
    fn metadata(id: FrameId, ordinal: u64, viewport_width: u32) -> CapturedFrame {
        CapturedFrame::new(
            id,
            session(),
            target(),
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(ordinal + 10),
            SessionTime::from_nanos(ordinal),
            ImageFormat::Png,
            krometrail_core::PixelDimensions::new(4, 4).unwrap(),
            krometrail_core::PixelDimensions::new(viewport_width, 4).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap()
    }
    fn source_frames() -> Vec<(CapturedFrame, Arc<[u8]>)> {
        vec![
            (metadata(frame_id(3), 1, 4), Arc::<[u8]>::from([1_u8, 2, 3])),
            (
                metadata(frame_id(4), 2, 4),
                Arc::<[u8]>::from([4_u8, 5, 6, 7]),
            ),
        ]
    }
    fn source_limits() -> SourceReadLimitsRequest {
        SourceReadLimitsRequest::new(2, 16, 32).unwrap()
    }
    fn output() -> OutputLimitsRequest {
        OutputLimitsRequest::new(1024, 1024, 1_000_000).unwrap()
    }
    fn labels() -> ArtifactLabelsRequest {
        ArtifactLabelsRequest::new(
            NonEmptyText::new("region").unwrap(),
            NonEmptyText::new("fixture").unwrap(),
        )
    }
    fn generic_generation() -> ArtifactGenerationRequest {
        let normalization =
            NormalizationRequest::new(None, Rgb8::new(0, 0, 0), AnalysisScale::Identity).unwrap();
        ArtifactGenerationRequest::new(
            range(),
            vec![],
            vec![
                ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
                    anchor: SessionTime::from_nanos(1),
                    tile_limit: 3,
                    noise_floor: 0,
                    normalization,
                    labels: labels(),
                    include_orientation: false,
                    output: output(),
                }),
                ArtifactGeneratorRequest::DifferenceMap(DifferenceMapRequest {
                    reference: FrameSelector::First,
                    frequency_mode: FrequencyMode::Count,
                    repeated_change_separation_nanos: None,
                    noise_floor: 0,
                    normalization,
                    canvas_background: Rgb8::new(0, 0, 0),
                    output: output(),
                }),
                ArtifactGeneratorRequest::RegionFilmstrip(RegionFilmstripRequest {
                    region: RegionDefinition::FixedSourceImage {
                        rect: SignedPixelRect::new(
                            0,
                            0,
                            NonZeroU32::new(1).unwrap(),
                            NonZeroU32::new(1).unwrap(),
                        )
                        .unwrap(),
                    },
                    mask: None,
                    anchor: SessionTime::from_nanos(1),
                    tile_limit: 2,
                    locator: None,
                    background: Rgb8::new(0, 0, 0),
                    padding: Rgb8::new(1, 2, 3),
                    display_scale: AnalysisScale::Identity,
                    labels: labels(),
                    output: output(),
                }),
                ArtifactGeneratorRequest::MotionHistory(MotionHistoryRequest {
                    reference: FrameSelector::Last,
                    noise_floor: 0,
                    normalization,
                    decay_peak: u16::MAX,
                    decay_half_life_ranks: 1,
                    reference_strength: 64,
                    accent: Rgb8::new(255, 0, 0),
                    outline: Rgb8::new(255, 255, 255),
                    labels: labels(),
                    output: output(),
                }),
            ],
            ArtifactFailurePolicy::RequireAll,
        )
        .unwrap()
    }
    fn region_request(
        region: krometrail_core::ProgressiveRegion,
    ) -> RegionFilmstripEvidenceRequest {
        RegionFilmstripEvidenceRequest::new(
            range(),
            region,
            vec![],
            SessionTime::from_nanos(1),
            2,
            Rgb8::new(0, 0, 0),
            Rgb8::new(1, 2, 3),
            AnalysisScale::Identity,
            labels(),
            output(),
        )
        .unwrap()
    }

    fn retention(pinned: u64) -> RetentionStatus {
        RetentionStatus::new(
            DiskBudgetBytes::new(10_000).unwrap(),
            StorageUsage::new(10, 0, 0, 0, 0, 0, 0).unwrap(),
            pinned,
            None,
            None,
            RecordingBudgetState::Available,
            false,
            false,
            0,
            0,
            0,
        )
        .unwrap()
    }
    fn pin_state(request: RetentionPinRequest) -> PinState {
        PinState::new(
            request,
            false,
            RangeEvidenceAvailability::Complete,
            PinProtectionScope::SourceSegmentsOnly,
            vec![],
            vec![],
            0,
            retention(0),
        )
        .unwrap()
    }

    fn artifact_read() -> ArtifactRead {
        let bytes: Arc<[u8]> = Arc::from([9_u8, 8, 7]);
        let dimensions = temporal_vision::PixelDimensions::new(1, 1).unwrap();
        let frame = temporal_vision::Frame::new(
            frame_id(3),
            temporal_vision::Timestamp::from_nanos(1),
            dimensions,
            temporal_vision::PixelFormat::Rgba8SrgbStraight,
            vec![0_u8; 4].into_boxed_slice(),
        )
        .unwrap();
        let sequence = temporal_vision::FrameSequence::<
            FrameId,
            krometrail_core::ArtifactMarkerId,
            krometrail_core::GapId,
            Box<[u8]>,
        >::new(vec![frame], vec![], vec![], None, None)
        .unwrap();
        let digest = Sha256Digest::digest(&bytes);
        let manifest = temporal_vision::ArtifactManifest::from_sequence(
            krometrail_core::ArtifactId::from_uuid(Uuid::from_u128(50)),
            ArtifactKind::RegionFilmstrip,
            temporal_vision::EvidenceClass::SourceDerived,
            temporal_vision::AlgorithmDescriptor::new("fixture", "1").unwrap(),
            &sequence,
            vec![frame_id(3)],
            vec![],
            temporal_vision::Parameters::default(),
            dimensions,
            temporal_vision::OutputHash::from_bytes(*digest.as_bytes()),
        )
        .unwrap();
        let handle = ArtifactEvidenceHandle::new(
            *manifest.artifact_id(),
            EvidenceScope::new(session(), target()).unwrap(),
            NonEmptyText::new("image/png").unwrap(),
            digest,
            bytes.len() as u64,
            manifest,
        )
        .unwrap();
        ArtifactRead::new(handle, bytes).unwrap()
    }

    struct FakeStore {
        frames: Vec<(CapturedFrame, Arc<[u8]>)>,
        artifact: ArtifactRead,
        calls: Mutex<Vec<&'static str>>,
    }
    impl FakeStore {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                frames: source_frames(),
                artifact: artifact_read(),
                calls: Mutex::new(vec![]),
            })
        }
        fn selected(
            &self,
            request: &SourceFramesRequest,
        ) -> Vec<(CapturedFrame, Arc<[u8]>, usize)> {
            let ids = match &request.selection {
                SourceFrameSelection::ResolvedOrder => request.range.frame_ids.clone(),
                SourceFrameSelection::Ids(ids) => ids.clone(),
            };
            ids.into_iter()
                .map(|id| {
                    let resolved = request
                        .range
                        .frame_ids
                        .iter()
                        .position(|candidate| *candidate == id)
                        .unwrap();
                    let (metadata, bytes) = self
                        .frames
                        .iter()
                        .find(|(metadata, _)| metadata.id() == id)
                        .unwrap();
                    (metadata.clone(), Arc::clone(bytes), resolved)
                })
                .collect()
        }
        fn handles(&self, request: &SourceFramesRequest) -> Vec<SourceFrameRead> {
            self.selected(request)
                .into_iter()
                .enumerate()
                .map(|(position, (metadata, bytes, resolved))| {
                    let handle = SourceFrameHandle::new(
                        metadata.id(),
                        EvidenceScope::new(session(), target()).unwrap(),
                        position as u32,
                        resolved as u32,
                        NonEmptyText::new("image/png").unwrap(),
                        Sha256Digest::digest(&bytes),
                        bytes.len() as u64,
                        metadata,
                    )
                    .unwrap();
                    SourceFrameRead::new(handle, bytes).unwrap()
                })
                .collect()
        }
    }
    impl FrameSource for FakeStore {
        fn list_source_frames(
            &self,
            request: SourceFramesRequest,
        ) -> PortFuture<'_, Result<SourceFrameList>> {
            self.calls.lock().unwrap().push("list");
            let range = request.range.clone();
            let frames = self
                .handles(&request)
                .into_iter()
                .map(|read| read.handle)
                .collect();
            Box::pin(std::future::ready(Ok(SourceFrameList { range, frames })))
        }
        fn fetch_source_frames(
            &self,
            request: SourceFramesRequest,
        ) -> PortFuture<'_, Result<SourceFrameBatch>> {
            self.calls.lock().unwrap().push("fetch");
            let range = request.range.clone();
            let frames = self.handles(&request);
            Box::pin(std::future::ready(Ok(SourceFrameBatch { range, frames })))
        }
        fn frames_by_id(&self, _: Vec<FrameId>) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
            panic!("unused")
        }
        fn frame_metadata_by_id(
            &self,
            ids: Vec<FrameId>,
        ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
            self.calls.lock().unwrap().push("metadata");
            let result = ids
                .into_iter()
                .map(|id| {
                    self.frames
                        .iter()
                        .find(|(frame, _)| frame.id() == id)
                        .unwrap()
                        .0
                        .clone()
                })
                .collect();
            Box::pin(std::future::ready(Ok(result)))
        }
        fn frames_in_range(
            &self,
            _: SessionId,
            _: TargetId,
            _: SessionRange,
        ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
            panic!("unused")
        }
        fn frames_in_ordinal_range(
            &self,
            _: SessionId,
            _: TargetId,
            _: CaptureOrdinal,
            _: CaptureOrdinal,
        ) -> PortFuture<'_, Result<Vec<EncodedFrame>>> {
            panic!("unused")
        }
        fn frame_metadata_in_range(
            &self,
            _: SessionId,
            _: TargetId,
            _: SessionRange,
        ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
            panic!("unused")
        }
        fn frame_metadata_in_ordinal_range(
            &self,
            _: SessionId,
            _: TargetId,
            _: CaptureOrdinal,
            _: CaptureOrdinal,
        ) -> PortFuture<'_, Result<Vec<CapturedFrame>>> {
            panic!("unused")
        }
        fn frame_availability(
            &self,
            _: SessionId,
            _: TargetId,
        ) -> PortFuture<'_, Result<FrameAvailability>> {
            panic!("unused")
        }
    }
    impl ArtifactStore for FakeStore {
        fn read_artifact(
            &self,
            _: krometrail_core::RetrieveArtifactRequest,
        ) -> PortFuture<'_, Result<ArtifactReadLookup>> {
            self.calls.lock().unwrap().push("retrieve");
            Box::pin(std::future::ready(Ok(ArtifactReadLookup::Available(
                Box::new(self.artifact.clone()),
            ))))
        }
        fn lookup_artifact(
            &self,
            _: ArtifactCacheKey,
            _: Vec<ArtifactSourceFingerprint>,
        ) -> PortFuture<'_, Result<ArtifactLookup>> {
            panic!("unused")
        }
        fn publish_artifact(
            &self,
            _: ArtifactPublication,
        ) -> PortFuture<'_, Result<ArtifactPublish>> {
            panic!("unused")
        }
        fn artifact(
            &self,
            _: krometrail_core::ArtifactId,
        ) -> PortFuture<'_, Result<Option<krometrail_core::StoredArtifact>>> {
            panic!("unused")
        }
    }
    impl krometrail_core::RetentionStore for FakeStore {
        fn pin_resolved_range(
            &self,
            request: RetentionPinRequest,
        ) -> PortFuture<'_, Result<ProgressivePinChange>> {
            self.calls.lock().unwrap().push("pin");
            Box::pin(std::future::ready(Ok(ProgressivePinChange {
                changed: true,
                state: pin_state(request),
            })))
        }
        fn unpin_resolved_range(
            &self,
            request: RetentionPinRequest,
        ) -> PortFuture<'_, Result<ProgressivePinChange>> {
            self.calls.lock().unwrap().push("unpin");
            Box::pin(std::future::ready(Ok(ProgressivePinChange {
                changed: true,
                state: pin_state(request),
            })))
        }
        fn query_pin_state(
            &self,
            request: RetentionPinRequest,
        ) -> PortFuture<'_, Result<PinState>> {
            self.calls.lock().unwrap().push("query");
            Box::pin(std::future::ready(Ok(pin_state(request))))
        }
        fn pin_range(
            &self,
            _: RetentionRange,
        ) -> PortFuture<'_, Result<krometrail_core::PinChange>> {
            panic!("unused")
        }
        fn unpin_range(
            &self,
            _: RetentionRange,
        ) -> PortFuture<'_, Result<krometrail_core::PinChange>> {
            panic!("unused")
        }
        fn enforce_budget(&self) -> PortFuture<'_, Result<RetentionStatus>> {
            panic!("unused")
        }
        fn status(&self) -> PortFuture<'_, Result<RetentionStatus>> {
            panic!("unused")
        }
        fn delete_session(&self, _: SessionId) -> PortFuture<'_, Result<SessionDeletion>> {
            panic!("unused")
        }
        fn wait_until_recording_allowed(&self) -> PortFuture<'_, Result<()>> {
            panic!("unused")
        }
    }

    struct FakeGeneration {
        calls: Mutex<Vec<ArtifactGenerationRequest>>,
    }
    impl FakeGeneration {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(vec![]),
            })
        }
    }
    impl ArtifactGeneration for FakeGeneration {
        fn generate(
            &self,
            request: ArtifactGenerationRequest,
            _: ArtifactGenerationContext,
        ) -> PortFuture<'_, Result<ArtifactGenerationResult>> {
            self.calls.lock().unwrap().push(request.clone());
            let result = ArtifactGenerationResult {
                range: request.range().clone(),
                epochs: vec![krometrail_core::VisualEpoch {
                    index: 0,
                    frame_ids: request.range().frame_ids.clone(),
                    image: krometrail_core::PixelDimensions::new(4, 4).unwrap(),
                    viewport: krometrail_core::PixelDimensions::new(4, 4).unwrap(),
                    device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
                }],
                outcomes: vec![],
            };
            Box::pin(std::future::ready(Ok(result)))
        }
    }

    struct Geometry {
        calls: AtomicUsize,
        error: bool,
    }

    struct ContradictoryGeometry;

    impl CurrentReferenceGeometry for ContradictoryGeometry {
        fn current_reference_geometry(
            &self,
            request: CurrentReferenceGeometryRequest,
        ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
            Box::pin(std::future::ready(Ok(ResolvedReferenceGeometry {
                session_id: SessionId::from_uuid(Uuid::from_u128(99)),
                target_id: request.reference.target_id,
                reference: request.reference,
                attachment_generation: 1,
                observed_at: ObservedTime::from_nanos(5),
                resolved_at: SessionTime::from_nanos(5),
                viewport_css_rect: krometrail_core::CssRect::new(
                    krometrail_core::CssPoint::new(0.0, 0.0).unwrap(),
                    krometrail_core::CssSize::new(1.0, 1.0).unwrap(),
                )
                .unwrap(),
            })))
        }
    }
    impl CurrentReferenceGeometry for Geometry {
        fn current_reference_geometry(
            &self,
            request: CurrentReferenceGeometryRequest,
        ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.error {
                return Box::pin(std::future::ready(Err(KrometrailError::new(
                    ErrorCode::StaleReference,
                    NonEmptyText::new("stale fixture reference").unwrap(),
                ))));
            }
            Box::pin(std::future::ready(ResolvedReferenceGeometry::new(
                request,
                request.reference.target_id,
                1,
                ObservedTime::from_nanos(5),
                SessionTime::from_nanos(5),
                krometrail_core::CssRect::new(
                    krometrail_core::CssPoint::new(-0.25, 1.1).unwrap(),
                    krometrail_core::CssSize::new(2.5, 2.2).unwrap(),
                )
                .unwrap(),
            )))
        }
    }

    fn test_service(
        store: &Arc<FakeStore>,
        generation: &Arc<FakeGeneration>,
    ) -> ProgressiveEvidenceService {
        ProgressiveEvidenceService::new(
            Arc::clone(store) as Arc<dyn ProgressiveEvidenceStore>,
            Arc::clone(generation) as Arc<dyn ArtifactGeneration>,
        )
    }

    #[tokio::test]
    async fn one_service_dispatches_all_eight_operations_and_preserves_source_order_and_bytes() {
        let store = FakeStore::new();
        let generation = FakeGeneration::new();
        let service = test_service(&store, &generation);
        let requests = vec![
            ProgressiveEvidenceRequest::RetrieveArtifact(
                krometrail_core::RetrieveArtifactRequest::new(
                    EvidenceScope::new(session(), target()).unwrap(),
                    store.artifact.handle.artifact_id,
                    16,
                )
                .unwrap(),
            ),
            ProgressiveEvidenceRequest::ListSourceFrames(
                SourceFramesRequest::new(
                    range(),
                    SourceFrameSelection::ResolvedOrder,
                    source_limits(),
                )
                .unwrap(),
            ),
            ProgressiveEvidenceRequest::FetchSourceFrames(
                SourceFramesRequest::new(
                    range(),
                    SourceFrameSelection::Ids(vec![frame_id(4), frame_id(3)]),
                    source_limits(),
                )
                .unwrap(),
            ),
            ProgressiveEvidenceRequest::GenerateArtifacts(
                GenerateArtifactsRequest::new(generic_generation()).unwrap(),
            ),
            ProgressiveEvidenceRequest::GenerateRegionFilmstrip(region_request(
                krometrail_core::ProgressiveRegion::SourcePixels {
                    rect: SignedPixelRect::new(
                        0,
                        0,
                        NonZeroU32::new(2).unwrap(),
                        NonZeroU32::new(2).unwrap(),
                    )
                    .unwrap(),
                    source_frame_id: frame_id(3),
                },
            )),
            ProgressiveEvidenceRequest::PinResolvedRange(
                ResolvedRangeEvidenceRequest::new(range()).unwrap(),
            ),
            ProgressiveEvidenceRequest::UnpinResolvedRange(
                ResolvedRangeEvidenceRequest::new(range()).unwrap(),
            ),
            ProgressiveEvidenceRequest::QueryPinState(
                ResolvedRangeEvidenceRequest::new(range()).unwrap(),
            ),
        ];
        for request in requests {
            let expected = request.kind();
            let result = service
                .execute(request, ProgressiveEvidenceContext::default())
                .await
                .unwrap();
            assert_eq!(result.kind(), expected);
            if let ProgressiveEvidenceResult::ListSourceFrames(result) = &result {
                assert_eq!(
                    result
                        .frames
                        .iter()
                        .map(|handle| handle.frame_id)
                        .collect::<Vec<_>>(),
                    range().frame_ids
                );
                let first = &result.frames[0];
                assert_eq!(first.media_type.as_str(), "image/png");
                assert_eq!(first.provenance.image().width(), 4);
                assert_eq!(first.provenance.viewport().width(), 4);
                assert_eq!(
                    first.provenance.observed_time(),
                    ObservedTime::from_nanos(11)
                );
                assert_eq!(first.provenance.session_time(), SessionTime::from_nanos(1));
                assert_eq!(
                    first.provenance.capture_ordinal(),
                    CaptureOrdinal::new(1).unwrap()
                );
                assert_eq!(first.encoded_byte_len, 3);
                assert_eq!(first.content_sha256, Sha256Digest::digest(&[1, 2, 3]));
            }
            if let ProgressiveEvidenceResult::FetchSourceFrames(result) = &result {
                assert_eq!(
                    result
                        .frames
                        .iter()
                        .map(|read| read.handle.frame_id)
                        .collect::<Vec<_>>(),
                    vec![frame_id(4), frame_id(3)]
                );
                assert_eq!(
                    result
                        .frames
                        .iter()
                        .map(|read| {
                            (read.handle.request_position, read.handle.resolved_position)
                        })
                        .collect::<Vec<_>>(),
                    vec![(0, 1), (1, 0)]
                );
                assert_eq!(result.frames[0].encoded_bytes(), [4, 5, 6, 7]);
            }
        }
        assert_eq!(
            store.calls.lock().unwrap().as_slice(),
            [
                "retrieve", "list", "fetch", "metadata", "pin", "unpin", "query"
            ]
        );
        let generated = generation.calls.lock().unwrap();
        assert_eq!(generated.len(), 2);
        assert_eq!(generated[0].generators().len(), 4);
        assert!(matches!(
            generated[1].generators(),
            [ArtifactGeneratorRequest::RegionFilmstrip(_)]
        ));
    }

    #[tokio::test]
    async fn service_revalidates_count_and_byte_limits_without_partial_results() {
        let store = FakeStore::new();
        let generation = FakeGeneration::new();
        let service = test_service(&store, &generation);
        let bypassed = SourceFramesRequest {
            range: range(),
            selection: SourceFrameSelection::ResolvedOrder,
            limits: SourceReadLimitsRequest::new(1, 16, 16).unwrap(),
        };
        let error = service
            .execute(
                ProgressiveEvidenceRequest::ListSourceFrames(bypassed),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(store.calls.lock().unwrap().is_empty());

        let too_small = SourceFramesRequest::new(
            range(),
            SourceFrameSelection::Ids(vec![frame_id(4)]),
            SourceReadLimitsRequest::new(1, 3, 3).unwrap(),
        )
        .unwrap();
        let error = service
            .execute(
                ProgressiveEvidenceRequest::FetchSourceFrames(too_small),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ResourceLimitExceeded);
    }

    #[tokio::test]
    async fn all_fixed_region_forms_map_once_with_rounding_mask_and_current_provenance() {
        let store = FakeStore::new();
        let generation = FakeGeneration::new();
        let service = test_service(&store, &generation);
        let rect = SignedPixelRect::new(
            -1,
            0,
            NonZeroU32::new(3).unwrap(),
            NonZeroU32::new(2).unwrap(),
        )
        .unwrap();
        let mask = BinaryMask::new(
            temporal_vision::PixelDimensions::new(4, 4).unwrap(),
            [0x00, 0x40],
        )
        .unwrap();
        let reference = krometrail_core::NodeReference {
            target_id: target(),
            generation: SnapshotGeneration::new(1).unwrap(),
            node_id: SnapshotNodeId::new(1).unwrap(),
        };
        let geometry = Arc::new(Geometry {
            calls: AtomicUsize::new(0),
            error: false,
        });
        let context = ProgressiveEvidenceContext {
            current_reference_geometry: Some(
                Arc::clone(&geometry) as Arc<dyn CurrentReferenceGeometry>
            ),
            ..ProgressiveEvidenceContext::default()
        };
        let regions = vec![
            krometrail_core::ProgressiveRegion::SourcePixels {
                rect,
                source_frame_id: frame_id(3),
            },
            krometrail_core::ProgressiveRegion::ViewportCss {
                rect: krometrail_core::CssRect::new(
                    krometrail_core::CssPoint::new(-0.25, 1.1).unwrap(),
                    krometrail_core::CssSize::new(2.5, 2.2).unwrap(),
                )
                .unwrap(),
                source_frame_id: frame_id(3),
            },
            krometrail_core::ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: frame_id(3),
                shape: krometrail_core::CallerRegionShape::Rect { rect },
            },
            krometrail_core::ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: frame_id(3),
                shape: krometrail_core::CallerRegionShape::Mask { mask: mask.clone() },
            },
            krometrail_core::ProgressiveRegion::CurrentReference {
                session_id: session(),
                reference,
                source_frame_id: frame_id(3),
            },
        ];
        let mut resolved = Vec::new();
        for region in regions {
            let result = service
                .execute(
                    ProgressiveEvidenceRequest::GenerateRegionFilmstrip(region_request(region)),
                    context.clone(),
                )
                .await
                .unwrap();
            let ProgressiveEvidenceResult::GenerateRegionFilmstrip(result) = result else {
                unreachable!()
            };
            resolved.push(result.region);
        }
        assert!(matches!(
            resolved[0].temporal_region,
            RegionDefinition::FixedSourceImage { rect: value } if value == rect
        ));
        assert!(matches!(
            resolved[1].temporal_region,
            RegionDefinition::FixedViewport { rect, .. }
                if (rect.x(), rect.y(), rect.width(), rect.height()) == (-1, 1, 4, 3)
        ));
        assert_eq!(resolved[3].mask.as_ref(), Some(&mask));
        assert!(matches!(
            resolved[3].temporal_region,
            RegionDefinition::FixedSourceImage { rect }
                if (rect.x(), rect.y(), rect.width(), rect.height()) == (1, 2, 1, 1)
        ));
        assert!(resolved[4].reference_geometry.is_some());
        assert_eq!(geometry.calls.load(Ordering::SeqCst), 1);
        let generated = generation.calls.lock().unwrap();
        assert!(
            generated
                .iter()
                .all(|request| request.failure_policy() == ArtifactFailurePolicy::RequireAll)
        );
        assert!(generated.iter().all(|request| matches!(
            request.generators(),
            [ArtifactGeneratorRequest::RegionFilmstrip(_)]
        )));
        let ArtifactGeneratorRequest::RegionFilmstrip(masked) = &generated[3].generators()[0]
        else {
            unreachable!()
        };
        assert_eq!(masked.mask.as_ref(), Some(&mask));
    }

    #[tokio::test]
    async fn region_scope_stale_current_and_multi_epoch_fail_before_generation() {
        let store = FakeStore::new();
        let generation = FakeGeneration::new();
        let service = test_service(&store, &generation);
        let reference = krometrail_core::NodeReference {
            target_id: target(),
            generation: SnapshotGeneration::new(1).unwrap(),
            node_id: SnapshotNodeId::new(1).unwrap(),
        };
        let stale = Arc::new(Geometry {
            calls: AtomicUsize::new(0),
            error: true,
        });
        let context = ProgressiveEvidenceContext {
            current_reference_geometry: Some(
                Arc::clone(&stale) as Arc<dyn CurrentReferenceGeometry>
            ),
            ..ProgressiveEvidenceContext::default()
        };
        let error = service
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(region_request(
                    krometrail_core::ProgressiveRegion::CurrentReference {
                        session_id: session(),
                        reference,
                        source_frame_id: frame_id(3),
                    },
                )),
                context,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleReference);
        assert_eq!(stale.calls.load(Ordering::SeqCst), 1);
        assert!(generation.calls.lock().unwrap().is_empty());

        let current_region = region_request(krometrail_core::ProgressiveRegion::CurrentReference {
            session_id: session(),
            reference,
            source_frame_id: frame_id(3),
        });
        let missing_geometry = service
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(current_region.clone()),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(missing_geometry.code, ErrorCode::InvalidLifecycleTransition);
        let contradictory = service
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(current_region),
                ProgressiveEvidenceContext {
                    current_reference_geometry: Some(Arc::new(ContradictoryGeometry)),
                    ..ProgressiveEvidenceContext::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(contradictory.code, ErrorCode::StaleReference);

        let wrong_scope = RegionFilmstripEvidenceRequest {
            range: range(),
            region: krometrail_core::ProgressiveRegion::CurrentReference {
                session_id: SessionId::from_uuid(Uuid::from_u128(99)),
                reference,
                source_frame_id: frame_id(3),
            },
            markers: vec![],
            anchor: SessionTime::from_nanos(1),
            tile_limit: 2,
            background: Rgb8::new(0, 0, 0),
            padding: Rgb8::new(1, 2, 3),
            display_scale: AnalysisScale::Identity,
            labels: labels(),
            output: output(),
        };
        let error = service
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(wrong_scope),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);

        let multi_epoch = Arc::new(FakeStore {
            frames: vec![
                (metadata(frame_id(3), 1, 4), Arc::<[u8]>::from([1_u8])),
                (metadata(frame_id(4), 2, 5), Arc::<[u8]>::from([2_u8])),
            ],
            artifact: artifact_read(),
            calls: Mutex::new(vec![]),
        });
        let multi_service = test_service(&multi_epoch, &generation);
        let error = multi_service
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(region_request(
                    krometrail_core::ProgressiveRegion::SourcePixels {
                        rect: SignedPixelRect::new(
                            0,
                            0,
                            NonZeroU32::new(1).unwrap(),
                            NonZeroU32::new(1).unwrap(),
                        )
                        .unwrap(),
                        source_frame_id: frame_id(3),
                    },
                )),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
}
