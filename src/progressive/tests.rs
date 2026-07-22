use std::{
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use krometrail_core::{
    AnalysisScale, ArtifactCacheDisposition, ArtifactFailurePolicy, ArtifactGeneration,
    ArtifactGenerationRequest, ArtifactGeneratorRequest, ArtifactHandle, ArtifactLabelsRequest,
    ArtifactOutcome, ArtifactStore, BrowserEvent, BrowserEventBatch, BrowserEventClass,
    BrowserEventId, BrowserEventOrdinal, BrowserEventPayload, BrowserEventSelector,
    BrowserEventSeverity, BrowserEventSink, BrowserEventSource, CallerRegionShape,
    CancellationSignal, CaptureOrdinal, CapturedFrame, CurrentReferenceGeometry,
    CurrentReferenceGeometryRequest, DeviceScaleFactor, EncodedFrame, ErrorCode, EvidenceScope,
    FrameId, FrameSource, GenerateArtifactsRequest, IdSource, IdValue, ImageFormat,
    KrometrailError, NodeReference, NonEmptyText, ObservedTime, OutputLimitsRequest,
    PixelDimensions, PortFuture, ProgressiveEvidence, ProgressiveEvidenceContext,
    ProgressiveEvidenceRequest, ProgressiveEvidenceResult, ProgressiveEvidenceStore,
    ProgressiveRegion, RangeResolutionOptions, RecordingSink, RegionFilmstripEvidenceRequest,
    RegionFilmstripRequest, ResolvedRange, ResolvedRangeEvidenceRequest, ResolvedReferenceGeometry,
    RetentionStore, SessionId, SessionRange, SessionTime, SnapshotGeneration, SnapshotNodeId,
    SourceFrameSelection, SourceFramesRequest, SourceReadLimitsRequest, TargetId, TargetLifecycle,
    TargetLifecycleEvent, TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use temporal_vision::{BinaryMask, RegionDefinition, Rgb8, SignedPixelRect};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::artifacts::{ArtifactWorkLimits, TemporalVisionArtifactService};

use super::ProgressiveEvidenceService;

const JPEG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgb.jpg");
const PNG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgba.png");

struct TestDirectory(PathBuf);

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct SequenceIds(AtomicU64);

impl IdSource for SequenceIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(u128::from(
            self.0.fetch_add(1, Ordering::Relaxed),
        )))
    }
}

struct QualificationFixture {
    root: PathBuf,
    store: Arc<RecordingStore>,
    progressive: ProgressiveEvidenceService,
    session: SessionId,
    target: TargetId,
    frame_ids: Vec<FrameId>,
    frame_bytes: Vec<&'static [u8]>,
    range: ResolvedRange,
    multi_epoch_range: ResolvedRange,
    _directory: TestDirectory,
}

impl QualificationFixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "krometrail-progressive-qualification-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let segments = root.join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: root.join("index.sqlite3"),
                segments_directory: segments.clone(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        );
        let writer = Arc::new(
            SegmentWriter::open(SegmentStoreConfig {
                directory: segments,
                // Keep the final source segment open. The pin test proves that the progressive
                // pin command flushes it before deriving segment-granular protection.
                rotation: RotationConfig {
                    max_duration: Duration::from_secs(60),
                    max_size: 1_000_000,
                },
            })
            .unwrap(),
        );
        let store = Arc::new(RecordingStore::new(writer, index, store_test_clock()).unwrap());
        let session = SessionId::from_uuid(Uuid::from_u128(10_000));
        let target = TargetId::from_uuid(Uuid::from_u128(10_001));
        let frame_ids = vec![frame_id(10_010), frame_id(10_011), frame_id(10_012)];
        let frame_bytes = vec![JPEG, PNG, PNG];
        for (position, (id, bytes)) in frame_ids.iter().zip(&frame_bytes).enumerate() {
            let ordinal = u64::try_from(position + 1).unwrap();
            let session_time = if position == 0 { 1 } else { 2 };
            store
                .append_frame(encoded_frame(
                    *id,
                    session,
                    target,
                    ordinal,
                    session_time,
                    if position == 0 {
                        ImageFormat::Jpeg
                    } else {
                        ImageFormat::Png
                    },
                    2,
                    bytes,
                ))
                .await
                .unwrap();
        }
        let different_epoch = frame_id(10_013);
        store
            .append_frame(encoded_frame(
                different_epoch,
                session,
                target,
                4,
                3,
                ImageFormat::Png,
                3,
                PNG,
            ))
            .await
            .unwrap();
        store
            .append_event_batch(
                BrowserEventBatch::new(session, vec![browser_event(10_100, session, target, 1, 1)])
                    .unwrap(),
            )
            .await
            .unwrap();

        let range = resolved(session, target, frame_ids.clone(), 1, 2);
        let mut multi_epoch_ids = frame_ids.clone();
        multi_epoch_ids.push(different_epoch);
        let multi_epoch_range = resolved(session, target, multi_epoch_ids, 1, 3);
        let artifacts = Arc::new(
            TemporalVisionArtifactService::new(
                Arc::clone(&store) as Arc<dyn FrameSource>,
                Arc::clone(&store) as Arc<dyn ArtifactStore>,
                Arc::new(SequenceIds(AtomicU64::new(20_000))),
                ArtifactWorkLimits::default(),
            )
            .unwrap(),
        );
        let progressive = ProgressiveEvidenceService::new(
            Arc::clone(&store) as Arc<dyn ProgressiveEvidenceStore>,
            artifacts as Arc<dyn ArtifactGeneration>,
        );
        Self {
            root: root.clone(),
            store,
            progressive,
            session,
            target,
            frame_ids,
            frame_bytes,
            range,
            multi_epoch_range,
            _directory: TestDirectory(root),
        }
    }

    fn region_request(&self, region: ProgressiveRegion) -> RegionFilmstripEvidenceRequest {
        region_request(self.range.clone(), region)
    }

    async fn region(
        &self,
        region: ProgressiveRegion,
        context: ProgressiveEvidenceContext,
    ) -> krometrail_core::RegionFilmstripEvidence {
        let result = self
            .progressive
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(self.region_request(region)),
                context,
            )
            .await
            .unwrap();
        let ProgressiveEvidenceResult::GenerateRegionFilmstrip(result) = result else {
            unreachable!()
        };
        *result
    }
}

fn frame_id(value: u128) -> FrameId {
    FrameId::from_uuid(Uuid::from_u128(value))
}

#[allow(clippy::too_many_arguments)]
fn encoded_frame(
    id: FrameId,
    session: SessionId,
    target: TargetId,
    ordinal: u64,
    session_time: u64,
    format: ImageFormat,
    viewport_width: u32,
    bytes: &[u8],
) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            id,
            session,
            target,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(ordinal + 10),
            SessionTime::from_nanos(session_time),
            format,
            PixelDimensions::new(2, 2).unwrap(),
            PixelDimensions::new(viewport_width, 2).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        bytes.to_vec(),
    )
    .unwrap()
}

fn resolved(
    session: SessionId,
    target: TargetId,
    frame_ids: Vec<FrameId>,
    start: u64,
    end: u64,
) -> ResolvedRange {
    let range =
        SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end)).unwrap();
    ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        frame_ids,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap()
}

fn output() -> OutputLimitsRequest {
    OutputLimitsRequest::new(4096, 4096, 16 * 1024 * 1024).unwrap()
}

fn labels() -> ArtifactLabelsRequest {
    ArtifactLabelsRequest::new(
        NonEmptyText::new("progressive region").unwrap(),
        NonEmptyText::new("schema-v5 qualification").unwrap(),
    )
}

fn region_request(
    range: ResolvedRange,
    region: ProgressiveRegion,
) -> RegionFilmstripEvidenceRequest {
    RegionFilmstripEvidenceRequest::new(
        range,
        region,
        vec![],
        SessionTime::from_nanos(2),
        3,
        Rgb8::new(4, 5, 6),
        Rgb8::new(250, 1, 249),
        AnalysisScale::Identity,
        labels(),
        output(),
    )
    .unwrap()
}

fn browser_event(
    id: u128,
    session: SessionId,
    target: TargetId,
    ordinal: u64,
    time: u64,
) -> BrowserEvent {
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(id)),
        session,
        target,
        1,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time + 20),
        BrowserEventSeverity::Info,
        BrowserEventPayload::TargetLifecycle(TargetLifecycleEvent::new(TargetLifecycle::Attached)),
    )
    .unwrap()
}

fn event_selector(session: SessionId, target: TargetId) -> BrowserEventSelector {
    BrowserEventSelector::new(
        session,
        target,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap(),
        Vec::<BrowserEventClass>::new(),
        BrowserEventSeverity::Debug,
    )
    .unwrap()
}

fn available(result: &krometrail_core::ArtifactGenerationResult) -> &ArtifactHandle {
    match &result.outcomes[0] {
        ArtifactOutcome::Available { artifact, .. } => artifact,
        ArtifactOutcome::Unavailable { error, .. } => panic!("artifact failed: {error}"),
    }
}

fn selected_mask(bits: u8) -> BinaryMask {
    BinaryMask::new(temporal_vision::PixelDimensions::new(2, 2).unwrap(), [bits]).unwrap()
}

#[derive(Default)]
struct AlwaysCancelled;

impl CancellationSignal for AlwaysCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(std::future::ready(()))
    }
}

struct ScriptedGeometry {
    calls: AtomicUsize,
}

impl CurrentReferenceGeometry for ScriptedGeometry {
    fn current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ResolvedReferenceGeometry>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(ResolvedReferenceGeometry::new(
            request,
            request.reference.target_id,
            7,
            ObservedTime::from_nanos(100),
            SessionTime::from_nanos(90),
            krometrail_core::CssRect::new(
                krometrail_core::CssPoint::new(-0.25, 0.25).unwrap(),
                krometrail_core::CssSize::new(1.5, 1.5).unwrap(),
            )
            .unwrap(),
        )))
    }
}

struct StaleGeometry;

impl CurrentReferenceGeometry for StaleGeometry {
    fn current_reference_geometry(
        &self,
        _: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ResolvedReferenceGeometry>> {
        Box::pin(std::future::ready(Err(KrometrailError::new(
            ErrorCode::StaleReference,
            NonEmptyText::new("scripted stale reference").unwrap(),
        ))))
    }
}

#[tokio::test]
async fn real_schema_v5_service_qualifies_source_regions_cache_and_corruption_lifetime() {
    let fixture = QualificationFixture::new().await;
    let total_bytes = fixture
        .frame_bytes
        .iter()
        .map(|bytes| bytes.len() as u64)
        .sum::<u64>();
    let max_item = fixture
        .frame_bytes
        .iter()
        .map(|bytes| bytes.len() as u64)
        .max()
        .unwrap();
    let exact_limits = SourceReadLimitsRequest::new(3, max_item, total_bytes).unwrap();
    let listed = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::ListSourceFrames(
                SourceFramesRequest::new(
                    fixture.range.clone(),
                    SourceFrameSelection::ResolvedOrder,
                    exact_limits,
                )
                .unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let ProgressiveEvidenceResult::ListSourceFrames(listed) = listed else {
        unreachable!()
    };
    assert_eq!(
        listed
            .frames
            .iter()
            .map(|frame| frame.frame_id)
            .collect::<Vec<_>>(),
        fixture.frame_ids
    );
    for (position, (handle, bytes)) in listed.frames.iter().zip(&fixture.frame_bytes).enumerate() {
        let ordinal = u64::try_from(position + 1).unwrap();
        assert_eq!(handle.request_position, position as u32);
        assert_eq!(handle.resolved_position, position as u32);
        assert_eq!(
            handle.scope,
            EvidenceScope::new(fixture.session, fixture.target).unwrap()
        );
        assert_eq!(handle.provenance.id(), fixture.frame_ids[position]);
        assert_eq!(
            handle.provenance.image(),
            PixelDimensions::new(2, 2).unwrap()
        );
        assert_eq!(
            handle.provenance.viewport(),
            PixelDimensions::new(2, 2).unwrap()
        );
        assert_eq!(handle.provenance.device_scale_factor().get(), 1.0);
        assert_eq!(
            handle.provenance.observed_time(),
            ObservedTime::from_nanos(ordinal + 10)
        );
        assert_eq!(
            handle.provenance.capture_ordinal(),
            CaptureOrdinal::new(ordinal).unwrap()
        );
        assert_eq!(handle.encoded_byte_len, bytes.len() as u64);
        assert_eq!(
            handle.content_sha256,
            krometrail_core::Sha256Digest::digest(bytes)
        );
        assert_eq!(
            handle.media_type.as_str(),
            if position == 0 {
                "image/jpeg"
            } else {
                "image/png"
            }
        );
    }

    let explicit_ids = vec![fixture.frame_ids[2], fixture.frame_ids[0]];
    let explicit_total = fixture.frame_bytes[2].len() + fixture.frame_bytes[0].len();
    let fetched = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::FetchSourceFrames(
                SourceFramesRequest::new(
                    fixture.range.clone(),
                    SourceFrameSelection::Ids(explicit_ids.clone()),
                    SourceReadLimitsRequest::new(2, max_item, explicit_total as u64).unwrap(),
                )
                .unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let ProgressiveEvidenceResult::FetchSourceFrames(fetched) = fetched else {
        unreachable!()
    };
    assert_eq!(
        fetched
            .frames
            .iter()
            .map(|frame| frame.handle.frame_id)
            .collect::<Vec<_>>(),
        explicit_ids
    );
    assert_eq!(
        fetched
            .frames
            .iter()
            .map(|frame| (
                frame.handle.request_position,
                frame.handle.resolved_position
            ))
            .collect::<Vec<_>>(),
        [(0, 2), (1, 0)]
    );
    assert_eq!(fetched.frames[0].encoded_bytes(), fixture.frame_bytes[2]);
    assert_eq!(fetched.frames[1].encoded_bytes(), fixture.frame_bytes[0]);

    let paged = SourceFramesRequest::new(
        fixture.range.clone(),
        SourceFrameSelection::ResolvedOrder,
        SourceReadLimitsRequest::new(2, max_item, total_bytes).unwrap(),
    )
    .unwrap();
    assert_eq!(paged.selected_frame_ids().len(), 2);
    let largest = fixture
        .frame_bytes
        .iter()
        .position(|bytes| bytes.len() as u64 == max_item)
        .unwrap();
    let item_limit_error = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::FetchSourceFrames(
                SourceFramesRequest::new(
                    fixture.range.clone(),
                    SourceFrameSelection::Ids(vec![fixture.frame_ids[largest]]),
                    SourceReadLimitsRequest::new(1, max_item - 1, total_bytes).unwrap(),
                )
                .unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(item_limit_error.code, ErrorCode::ResourceLimitExceeded);
    let total_limit_error = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::ListSourceFrames(
                SourceFramesRequest::new(
                    fixture.range.clone(),
                    SourceFrameSelection::ResolvedOrder,
                    SourceReadLimitsRequest::new(3, max_item, total_bytes - 1).unwrap(),
                )
                .unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(total_limit_error.code, ErrorCode::ResourceLimitExceeded);

    let outside = SignedPixelRect::new(
        -1,
        -1,
        NonZeroU32::new(4).unwrap(),
        NonZeroU32::new(4).unwrap(),
    )
    .unwrap();
    let outside_result = fixture
        .region(
            ProgressiveRegion::SourcePixels {
                rect: outside,
                source_frame_id: fixture.frame_ids[1],
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    assert!(matches!(
        outside_result.region.temporal_region,
        RegionDefinition::FixedSourceImage { rect } if rect == outside
    ));
    assert!(
        available(&outside_result.generation)
            .manifest
            .selected_frame_ids()
            .contains(&fixture.frame_ids[1])
    );
    assert!(
        available(&outside_result.generation)
            .manifest
            .parameters()
            .get("padding_rgb8")
            .is_some()
    );

    let viewport = fixture
        .region(
            ProgressiveRegion::ViewportCss {
                rect: krometrail_core::CssRect::new(
                    krometrail_core::CssPoint::new(-0.25, 0.25).unwrap(),
                    krometrail_core::CssSize::new(1.5, 1.5).unwrap(),
                )
                .unwrap(),
                source_frame_id: fixture.frame_ids[1],
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    assert!(matches!(
        viewport.region.temporal_region,
        RegionDefinition::FixedViewport { rect, .. }
            if (rect.x(), rect.y(), rect.width(), rect.height()) == (-1, 0, 3, 2)
    ));

    let selected_rect = fixture
        .region(
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[1],
                shape: CallerRegionShape::Rect {
                    rect: SignedPixelRect::new(
                        0,
                        0,
                        NonZeroU32::new(1).unwrap(),
                        NonZeroU32::new(1).unwrap(),
                    )
                    .unwrap(),
                },
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    assert!(selected_rect.region.mask.is_none());

    let mask = selected_mask(0x90);
    let masked = fixture
        .region(
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[1],
                shape: CallerRegionShape::Mask { mask: mask.clone() },
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    let masked_handle = available(&masked.generation).clone();
    assert_eq!(masked.region.mask.as_ref(), Some(&mask));
    assert_eq!(masked_handle.manifest.mask(), Some(&mask));
    assert_eq!(masked_handle.cache, ArtifactCacheDisposition::Generated);

    let generic = ArtifactGenerationRequest::new(
        fixture.range.clone(),
        vec![],
        vec![ArtifactGeneratorRequest::RegionFilmstrip(
            RegionFilmstripRequest {
                region: RegionDefinition::FixedSourceImage {
                    rect: SignedPixelRect::new(
                        0,
                        0,
                        NonZeroU32::new(2).unwrap(),
                        NonZeroU32::new(2).unwrap(),
                    )
                    .unwrap(),
                },
                mask: Some(mask.clone()),
                anchor: SessionTime::from_nanos(2),
                tile_limit: 3,
                locator: Some(fixture.frame_ids[1]),
                background: Rgb8::new(4, 5, 6),
                padding: Rgb8::new(250, 1, 249),
                display_scale: AnalysisScale::Identity,
                labels: labels(),
                output: output(),
            },
        )],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let generic = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::GenerateArtifacts(
                GenerateArtifactsRequest::new(generic).unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let ProgressiveEvidenceResult::GenerateArtifacts(generic) = generic else {
        unreachable!()
    };
    assert_eq!(available(&generic).artifact_id, masked_handle.artifact_id);
    assert_eq!(available(&generic).cache, ArtifactCacheDisposition::Hit);

    let repeated = fixture
        .region(
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[1],
                shape: CallerRegionShape::Mask { mask: mask.clone() },
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    assert_eq!(
        available(&repeated.generation).artifact_id,
        masked_handle.artifact_id
    );
    assert_eq!(
        available(&repeated.generation).cache,
        ArtifactCacheDisposition::Hit
    );

    let retrieve = krometrail_core::RetrieveArtifactRequest::new(
        EvidenceScope::new(fixture.session, fixture.target).unwrap(),
        masked_handle.artifact_id,
        masked_handle.encoded_byte_len,
    )
    .unwrap();
    let read = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::RetrieveArtifact(retrieve.clone()),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let ProgressiveEvidenceResult::RetrieveArtifact(read) = read else {
        unreachable!()
    };
    assert_eq!(read.handle.artifact_id, masked_handle.artifact_id);
    assert_eq!(
        read.handle.scope,
        EvidenceScope::new(fixture.session, fixture.target).unwrap()
    );
    assert_eq!(read.handle.provenance, masked_handle.manifest);
    assert_eq!(
        read.encoded_bytes().len() as u64,
        masked_handle.encoded_byte_len
    );
    assert_eq!(
        krometrail_core::Sha256Digest::digest(read.encoded_bytes()),
        read.handle.content_sha256
    );
    let too_small = krometrail_core::RetrieveArtifactRequest::new(
        retrieve.scope,
        retrieve.artifact_id,
        masked_handle.encoded_byte_len - 1,
    )
    .unwrap();
    assert_eq!(
        fixture
            .progressive
            .execute(
                ProgressiveEvidenceRequest::RetrieveArtifact(too_small),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimitExceeded
    );

    std::fs::write(
        fixture
            .root
            .join("artifacts")
            .join(format!("{}.png", masked_handle.artifact_id)),
        b"corrupt artifact",
    )
    .unwrap();
    assert_eq!(
        fixture
            .progressive
            .execute(
                ProgressiveEvidenceRequest::RetrieveArtifact(retrieve),
                ProgressiveEvidenceContext::default(),
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::EvidenceInvalidated
    );
    let regenerated = fixture
        .region(
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[1],
                shape: CallerRegionShape::Mask { mask: mask.clone() },
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    assert_eq!(
        available(&regenerated.generation).cache,
        ArtifactCacheDisposition::Generated,
        "typed retrieval already invalidated and removed the corrupt cache row"
    );

    let different_mask = selected_mask(0x80);
    let different = fixture
        .region(
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[1],
                shape: CallerRegionShape::Mask {
                    mask: different_mask,
                },
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    assert_ne!(
        available(&different.generation).artifact_id,
        available(&regenerated.generation).artifact_id
    );

    let reference = NodeReference {
        target_id: fixture.target,
        generation: SnapshotGeneration::new(3).unwrap(),
        node_id: SnapshotNodeId::new(4).unwrap(),
    };
    let geometry = Arc::new(ScriptedGeometry {
        calls: AtomicUsize::new(0),
    });
    let current = fixture
        .region(
            ProgressiveRegion::CurrentReference {
                session_id: fixture.session,
                reference,
                source_frame_id: fixture.frame_ids[1],
            },
            ProgressiveEvidenceContext {
                current_reference_geometry: Some(
                    Arc::clone(&geometry) as Arc<dyn CurrentReferenceGeometry>
                ),
                ..ProgressiveEvidenceContext::default()
            },
        )
        .await;
    assert_eq!(geometry.calls.load(Ordering::SeqCst), 1);
    assert!(current.region.reference_geometry.is_some());
    assert!(matches!(
        current.region.temporal_region,
        RegionDefinition::FixedViewport { rect, .. }
            if (rect.x(), rect.y(), rect.width(), rect.height()) == (-1, 0, 3, 2)
    ));

    let stale = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::GenerateRegionFilmstrip(fixture.region_request(
                ProgressiveRegion::CurrentReference {
                    session_id: fixture.session,
                    reference,
                    source_frame_id: fixture.frame_ids[1],
                },
            )),
            ProgressiveEvidenceContext {
                current_reference_geometry: Some(Arc::new(StaleGeometry)),
                ..ProgressiveEvidenceContext::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::StaleReference);

    let wrong_scope = RegionFilmstripEvidenceRequest::new(
        fixture.range.clone(),
        ProgressiveRegion::CurrentReference {
            session_id: fixture.session,
            reference: NodeReference {
                target_id: TargetId::from_uuid(Uuid::from_u128(99_999)),
                ..reference
            },
            source_frame_id: fixture.frame_ids[1],
        },
        vec![],
        SessionTime::from_nanos(2),
        3,
        Rgb8::new(4, 5, 6),
        Rgb8::new(250, 1, 249),
        AnalysisScale::Identity,
        labels(),
        output(),
    )
    .unwrap_err();
    assert_eq!(wrong_scope.code, ErrorCode::InvalidInput);

    let epoch_error = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::GenerateRegionFilmstrip(region_request(
                fixture.multi_epoch_range.clone(),
                ProgressiveRegion::SourcePixels {
                    rect: SignedPixelRect::new(
                        0,
                        0,
                        NonZeroU32::new(1).unwrap(),
                        NonZeroU32::new(1).unwrap(),
                    )
                    .unwrap(),
                    source_frame_id: fixture.frame_ids[0],
                },
            )),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(epoch_error.code, ErrorCode::InvalidInput);

    let wrong_size = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::GenerateRegionFilmstrip(
                fixture.region_request(ProgressiveRegion::SelectedFromSourceFrame {
                    source_frame_id: fixture.frame_ids[0],
                    shape: CallerRegionShape::Mask {
                        mask: BinaryMask::new(
                            temporal_vision::PixelDimensions::new(1, 1).unwrap(),
                            [0x80],
                        )
                        .unwrap(),
                    },
                }),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(wrong_size.code, ErrorCode::InvalidInput);
    let all_zero =
        BinaryMask::new(temporal_vision::PixelDimensions::new(2, 2).unwrap(), [0_u8]).unwrap();
    assert_eq!(
        RegionFilmstripEvidenceRequest::new(
            fixture.range.clone(),
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[0],
                shape: CallerRegionShape::Mask { mask: all_zero },
            },
            vec![],
            SessionTime::from_nanos(2),
            3,
            Rgb8::new(4, 5, 6),
            Rgb8::new(250, 1, 249),
            AnalysisScale::Identity,
            labels(),
            output(),
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidInput
    );
    assert!(BinaryMask::new(temporal_vision::PixelDimensions::new(2, 2).unwrap(), [0x81]).is_err());
    let mut oversized_bits = vec![0_u8; 8193_usize.div_ceil(8)];
    oversized_bits[0] = 0x80;
    let oversized = BinaryMask::new(
        temporal_vision::PixelDimensions::new(8193, 1).unwrap(),
        oversized_bits,
    )
    .unwrap();
    assert_eq!(
        RegionFilmstripEvidenceRequest::new(
            fixture.range.clone(),
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[0],
                shape: CallerRegionShape::Mask { mask: oversized },
            },
            vec![],
            SessionTime::from_nanos(2),
            3,
            Rgb8::new(4, 5, 6),
            Rgb8::new(250, 1, 249),
            AnalysisScale::Identity,
            labels(),
            output(),
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidInput
    );

    let artifact_bytes_before_cancel = fixture.store.status().await.unwrap().usage.artifact_bytes;
    for context in [
        ProgressiveEvidenceContext {
            cancellation: Some(Arc::new(AlwaysCancelled)),
            ..ProgressiveEvidenceContext::default()
        },
        ProgressiveEvidenceContext {
            deadline: Some(Instant::now()),
            ..ProgressiveEvidenceContext::default()
        },
    ] {
        assert_eq!(
            fixture
                .progressive
                .execute(
                    ProgressiveEvidenceRequest::GenerateRegionFilmstrip(
                        fixture.region_request(ProgressiveRegion::SourcePixels {
                            rect: SignedPixelRect::new(
                                0,
                                0,
                                NonZeroU32::new(2).unwrap(),
                                NonZeroU32::new(1).unwrap(),
                            )
                            .unwrap(),
                            source_frame_id: fixture.frame_ids[0],
                        },)
                    ),
                    context,
                )
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
    }
    assert_eq!(
        fixture.store.status().await.unwrap().usage.artifact_bytes,
        artifact_bytes_before_cancel
    );
}

#[tokio::test]
async fn real_service_pin_and_deletion_report_final_cross_subsystem_truth() {
    let fixture = QualificationFixture::new().await;
    let artifact = fixture
        .region(
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: fixture.frame_ids[1],
                shape: CallerRegionShape::Rect {
                    rect: SignedPixelRect::new(
                        0,
                        0,
                        NonZeroU32::new(1).unwrap(),
                        NonZeroU32::new(1).unwrap(),
                    )
                    .unwrap(),
                },
            },
            ProgressiveEvidenceContext::default(),
        )
        .await;
    let artifact = available(&artifact.generation).clone();

    let full = ResolvedRangeEvidenceRequest::new(fixture.range.clone()).unwrap();
    let pinned = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::PinResolvedRange(full.clone()),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let ProgressiveEvidenceResult::PinResolvedRange(pinned) = pinned else {
        unreachable!()
    };
    assert!(pinned.changed);
    assert!(pinned.state.exact_pin_active);
    assert!(!pinned.state.protected_segments.is_empty());
    assert!(pinned.state.protected_segments.iter().any(|segment| {
        segment.retained_range.start() <= fixture.range.resolved_range.start()
            && segment.retained_range.end() >= fixture.range.resolved_range.end()
    }));
    assert_eq!(
        pinned.state.pinned_usage_bytes,
        pinned.state.retention.pinned_usage_bytes
    );

    let repeated = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::PinResolvedRange(full.clone()),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    assert!(matches!(
        repeated,
        ProgressiveEvidenceResult::PinResolvedRange(change) if !change.changed
    ));

    let overlap_range = resolved(
        fixture.session,
        fixture.target,
        fixture.frame_ids[1..].to_vec(),
        2,
        2,
    );
    let overlap = ResolvedRangeEvidenceRequest::new(overlap_range).unwrap();
    fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::PinResolvedRange(overlap.clone()),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let removed = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::UnpinResolvedRange(full.clone()),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    let ProgressiveEvidenceResult::UnpinResolvedRange(removed) = removed else {
        unreachable!()
    };
    assert!(removed.changed);
    assert!(!removed.state.exact_pin_active);
    assert!(!removed.state.protected_segments.is_empty());
    let repeated_unpin = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::UnpinResolvedRange(full),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    assert!(matches!(
        repeated_unpin,
        ProgressiveEvidenceResult::UnpinResolvedRange(change) if !change.changed
    ));
    let overlap_state = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::QueryPinState(overlap.clone()),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap();
    assert!(matches!(
        overlap_state,
        ProgressiveEvidenceResult::QueryPinState(state) if state.exact_pin_active
    ));

    fixture.store.delete_session(fixture.session).await.unwrap();
    let source_after_delete = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::FetchSourceFrames(
                SourceFramesRequest::new(
                    fixture.range.clone(),
                    SourceFrameSelection::Ids(vec![fixture.frame_ids[0]]),
                    SourceReadLimitsRequest::new(1, 32 * 1024 * 1024, 32 * 1024 * 1024).unwrap(),
                )
                .unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(source_after_delete.code, ErrorCode::NotFound);
    let artifact_after_delete = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::RetrieveArtifact(
                krometrail_core::RetrieveArtifactRequest::new(
                    EvidenceScope::new(fixture.session, fixture.target).unwrap(),
                    artifact.artifact_id,
                    artifact.encoded_byte_len,
                )
                .unwrap(),
            ),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(artifact_after_delete.code, ErrorCode::NotFound);
    let pin_after_delete = fixture
        .progressive
        .execute(
            ProgressiveEvidenceRequest::QueryPinState(overlap),
            ProgressiveEvidenceContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(pin_after_delete.code, ErrorCode::NotFound);
    assert_eq!(
        fixture
            .store
            .count_events(event_selector(fixture.session, fixture.target))
            .await
            .unwrap(),
        0
    );
}

struct BlockingGeometry {
    reached: Arc<Semaphore>,
    release: Arc<Semaphore>,
}

impl CurrentReferenceGeometry for BlockingGeometry {
    fn current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ResolvedReferenceGeometry>> {
        let reached = Arc::clone(&self.reached);
        let release = Arc::clone(&self.release);
        Box::pin(async move {
            reached.add_permits(1);
            let permit = release
                .acquire()
                .await
                .expect("release semaphore remains open");
            permit.forget();
            ResolvedReferenceGeometry::new(
                request,
                request.reference.target_id,
                8,
                ObservedTime::from_nanos(200),
                SessionTime::from_nanos(190),
                krometrail_core::CssRect::new(
                    krometrail_core::CssPoint::new(0.0, 0.0).unwrap(),
                    krometrail_core::CssSize::new(1.0, 1.0).unwrap(),
                )
                .unwrap(),
            )
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn blocked_current_geometry_leaves_frame_and_schema_v5_event_persistence_available() {
    let fixture = QualificationFixture::new().await;
    let reached = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let geometry: Arc<dyn CurrentReferenceGeometry> = Arc::new(BlockingGeometry {
        reached: Arc::clone(&reached),
        release: Arc::clone(&release),
    });
    let request = fixture.region_request(ProgressiveRegion::CurrentReference {
        session_id: fixture.session,
        reference: NodeReference {
            target_id: fixture.target,
            generation: SnapshotGeneration::new(5).unwrap(),
            node_id: SnapshotNodeId::new(6).unwrap(),
        },
        source_frame_id: fixture.frame_ids[1],
    });
    let service = fixture.progressive.clone();
    let task = tokio::spawn(async move {
        service
            .execute(
                ProgressiveEvidenceRequest::GenerateRegionFilmstrip(request),
                ProgressiveEvidenceContext {
                    current_reference_geometry: Some(geometry),
                    ..ProgressiveEvidenceContext::default()
                },
            )
            .await
    });
    let reached_permit = reached.acquire().await.unwrap();
    reached_permit.forget();

    let persisted_frame = frame_id(88_001);
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.store.append_frame(encoded_frame(
            persisted_frame,
            fixture.session,
            fixture.target,
            5,
            4,
            ImageFormat::Png,
            2,
            PNG,
        )),
    )
    .await
    .expect("frame persistence must not wait for browser geometry")
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.store.append_event_batch(
            BrowserEventBatch::new(
                fixture.session,
                vec![browser_event(88_002, fixture.session, fixture.target, 2, 4)],
            )
            .unwrap(),
        ),
    )
    .await
    .expect("schema-v5 event persistence must not wait for browser geometry")
    .unwrap();

    release.add_permits(1);
    assert!(matches!(
        task.await.unwrap().unwrap(),
        ProgressiveEvidenceResult::GenerateRegionFilmstrip(_)
    ));
    assert_eq!(
        fixture
            .store
            .frames_by_id(vec![persisted_frame])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        persisted_frame
    );
    assert_eq!(
        fixture
            .store
            .count_events(event_selector(fixture.session, fixture.target))
            .await
            .unwrap(),
        2
    );
}

fn store_test_clock() -> std::sync::Arc<dyn krometrail_core::MonotonicClock> {
    struct Fixed;
    impl krometrail_core::MonotonicClock for Fixed {
        fn now(&self) -> krometrail_core::ObservedTime {
            krometrail_core::ObservedTime::from_nanos(0)
        }
    }
    std::sync::Arc::new(Fixed)
}
