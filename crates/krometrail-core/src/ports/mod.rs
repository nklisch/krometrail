//! Runtime-neutral infrastructure ports.
//!
//! Implementations belong to sibling adapter crates. This module contains only
//! domain-owned values and `std` traits so the core remains usable with any
//! executor or transport implementation.

pub mod artifacts;
pub mod browser;
pub mod browser_events;
pub mod catalog;
pub mod clock;
pub mod frames;
pub mod gaps;
pub mod ids;
pub mod progressive;
pub mod range;
pub mod recording;
pub mod retention;
pub mod timeline;
pub mod video;

/// The allocation is paid at an infrastructure boundary, keeping core traits
/// object-safe without selecting an async runtime or procedural macro.
pub type PortFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub use artifacts::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactReadLookup, ArtifactSourceFingerprint, ArtifactStore, StoredArtifact,
};
pub use browser::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserFailureKind, BrowserFocusPolicy,
    BrowserOperationContext, BrowserPageTargets, BrowserSessionEvents, BrowserSessionPort,
    CancellationSignal, CurrentReferenceGeometry, CurrentReferenceGeometryRequest, EveryNthFrame,
    LaunchBrowser, MAX_EVERY_NTH_FRAME, MIN_EVERY_NTH_FRAME, ManagedProfile,
    ResolvedReferenceGeometry,
};
pub use browser_events::{
    BrowserEventCursor, BrowserEventSelector, BrowserEventSink, BrowserEventSource,
    BrowserEventUnavailableRange, BrowserEventUnavailableReason, CaptureStatusSamples,
    DEFAULT_EVENT_PAGE_ROWS, EventCandidateLimit, EventPageLimit, MAX_CAPTURE_STATUS_SAMPLES,
    MAX_EVENT_CANDIDATE_ROWS, MAX_EVENT_PAGE_ROWS, MAX_EVENT_UNAVAILABLE_RANGES,
};
pub use catalog::RecordingCatalog;
pub use clock::{MonotonicClock, WallClock};
pub use frames::FrameSource;
pub use gaps::CaptureGapStore;
pub use ids::IdSource;
pub use progressive::ProgressiveEvidenceStore;
pub use range::{
    InteractionAnchorSource, InteractionEvidenceSink, InteractionRecordSource, TimelineAnchorSource,
};
pub use recording::RecordingSink;
pub use retention::RetentionStore;
pub use timeline::{
    MAX_TIMELINE_RANGE_ROWS, TimelineRangeQuery, TimelineRangeSlice, TimelineStore,
};
pub use video::{
    MAX_VIDEO_ENCODER_LABEL_BYTES, TemporalVideoEncoder, VideoEncodeFrame, VideoEncodeRequest,
    VideoEncodedClip, VideoEncoderIdentity, VideoEncodingContext, VideoEncodingProfile,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserCompatibility, BrowserOwnership, BrowserProduct, BrowserProductVersion,
        BrowserSessionEvent, BrowserSessionState, BrowserStopOutcome, BrowserVersion, ByteOffset,
        CaptureGap, CaptureGapReason, CapturedFrame, DeviceScaleFactor, EncodedFrame, ErrorCode,
        FrameAddress, ImageFormat, ObservationKind, ObservationPayloadRef, ObservedTime,
        PageTarget, PixelDimensions, ProfileIdentity, ProfileRef, RendererCapability, SegmentId,
        SessionId, SessionOrigin, SessionRange, SessionTime, SnapshotGeneration, SnapshotNodeId,
        SourceTime, SupervisedTarget, TargetId, TargetLifecycle, TargetVisibility,
        TimelineObservation,
    };
    use std::{
        collections::VecDeque,
        num::NonZeroU64,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
        task::{Context, Poll},
        time::{Duration, SystemTime},
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn block_on<T>(future: PortFuture<'_, T>) -> T {
        let mut context = Context::from_waker(std::task::Waker::noop());
        let mut future = future;
        loop {
            match Pin::new(&mut future).poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[derive(Debug)]
    struct FakeClocks;

    impl MonotonicClock for FakeClocks {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(42)
        }
    }

    impl WallClock for FakeClocks {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(7)
        }
    }

    #[derive(Debug)]
    struct FakeIds {
        values: Mutex<VecDeque<crate::IdValue>>,
    }

    impl IdSource for FakeIds {
        fn next(&self) -> crate::IdValue {
            self.values
                .lock()
                .unwrap()
                .pop_front()
                .expect("test ID available")
        }
    }

    #[derive(Debug)]
    struct ClosedEvents;

    impl BrowserSessionEvents for ClosedEvents {
        fn next(&mut self) -> PortFuture<'_, crate::Result<Option<BrowserSessionEvent>>> {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    #[derive(Debug)]
    struct FakeBrowserSession {
        compatibility: BrowserCompatibility,
        profile: ProfileRef,
        targets: Vec<SupervisedTarget>,
        session_id: SessionId,
        session_origin: SessionOrigin,
        fail: bool,
    }

    impl BrowserSessionPort for FakeBrowserSession {
        fn session_origin(&self) -> SessionOrigin {
            self.session_origin
        }

        fn status(&self) -> PortFuture<'_, crate::Result<crate::BrowserStatus>> {
            let ownership = match self.profile {
                ProfileRef::Managed(_) => BrowserOwnership::Managed,
                ProfileRef::External => BrowserOwnership::Attached,
            };
            let pages = if self.fail {
                Vec::new()
            } else {
                self.targets
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(index, target)| crate::PageStatus {
                        target,
                        selected: index == 0,
                    })
                    .collect()
            };
            let selected = pages.first().map(|page| page.target.target.id());
            let status = crate::BrowserStatus::new(
                self.session_id,
                if self.fail {
                    BrowserSessionState::Ended
                } else {
                    BrowserSessionState::Ready
                },
                ownership,
                self.profile.clone(),
                self.compatibility.clone(),
                selected,
                pages,
                Vec::new(),
                crate::RetentionStatus::empty(crate::DiskBudgetBytes::default()),
                EveryNthFrame::default(),
            );
            Box::pin(std::future::ready(status))
        }

        fn subscribe(&self) -> PortFuture<'_, crate::Result<Box<dyn BrowserSessionEvents>>> {
            Box::pin(std::future::ready(Ok(
                Box::new(ClosedEvents) as Box<dyn BrowserSessionEvents>
            )))
        }

        fn execute(
            &self,
            _request: crate::BrowserOperationRequest,
            _context: BrowserOperationContext,
        ) -> PortFuture<'_, crate::Result<crate::BrowserOperationResult>> {
            Box::pin(std::future::ready(Err(crate::KrometrailError::new(
                ErrorCode::Unsupported,
                crate::NonEmptyText::new("fake browser operation is not configured").unwrap(),
            ))))
        }

        fn stop(&self) -> PortFuture<'_, crate::Result<BrowserStopOutcome>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::ShutdownIncomplete,
                    crate::NonEmptyText::new("browser shutdown was incomplete").unwrap(),
                ))
            } else {
                Ok(match self.profile {
                    ProfileRef::Managed(_) => BrowserStopOutcome::ManagedBrowserClosed,
                    ProfileRef::External => BrowserStopOutcome::Detached,
                })
            };
            Box::pin(std::future::ready(result))
        }
    }

    struct FakeBrowserConnector {
        session: Arc<dyn BrowserSessionPort>,
        fail: bool,
    }

    impl BrowserConnector for FakeBrowserConnector {
        fn installations(&self) -> PortFuture<'_, crate::Result<Vec<crate::BrowserInstallation>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }

        fn connect(
            &self,
            _request: BrowserConnectRequest,
        ) -> PortFuture<'_, crate::Result<Arc<dyn BrowserSessionPort>>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::BrowserNotFound,
                    crate::NonEmptyText::new("browser connection is unavailable").unwrap(),
                ))
            } else {
                Ok(Arc::clone(&self.session))
            };
            Box::pin(std::future::ready(result))
        }
    }

    #[derive(Debug, Default)]
    struct FakeRecording {
        frames: Mutex<Vec<EncodedFrame>>,
        gaps: Mutex<Vec<CaptureGap>>,
        flushes: Mutex<Vec<SessionId>>,
        next_offset: AtomicU64,
        fail: bool,
    }

    impl RecordingSink for FakeRecording {
        fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, crate::Result<FrameAddress>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("frame persistence failed").unwrap(),
                ))
            } else {
                self.frames.lock().unwrap().push(frame);
                Ok(FrameAddress::new(
                    SegmentId::from_uuid(UUID.parse().unwrap()),
                    ByteOffset::new(self.next_offset.fetch_add(1, Ordering::Relaxed) + 1),
                ))
            };
            Box::pin(async move { result })
        }

        fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, crate::Result<()>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("gap persistence failed").unwrap(),
                ))
            } else {
                self.gaps.lock().unwrap().push(gap);
                Ok(())
            };
            Box::pin(async move { result })
        }

        fn flush(&self, session_id: SessionId) -> PortFuture<'_, crate::Result<()>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("recording flush failed").unwrap(),
                ))
            } else {
                self.flushes.lock().unwrap().push(session_id);
                Ok(())
            };
            Box::pin(async move { result })
        }
    }

    #[derive(Debug, Default)]
    struct FakeTimeline {
        observations: Mutex<Vec<TimelineObservation>>,
        fail: bool,
    }

    impl TimelineStore for FakeTimeline {
        fn append(&self, observation: TimelineObservation) -> PortFuture<'_, crate::Result<()>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("timeline append failed").unwrap(),
                ))
            } else {
                self.observations.lock().unwrap().push(observation);
                Ok(())
            };
            Box::pin(async move { result })
        }

        fn range(
            &self,
            session_id: SessionId,
            target_id: TargetId,
            range: SessionRange,
        ) -> PortFuture<'_, crate::Result<Vec<TimelineObservation>>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("timeline range failed").unwrap(),
                ))
            } else {
                Ok(self
                    .observations
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|observation| {
                        observation.session_id() == session_id
                            && observation.target_id() == target_id
                            && range.contains(observation.session_time())
                    })
                    .cloned()
                    .collect())
            };
            Box::pin(async move { result })
        }

        fn selected_range(
            &self,
            query: crate::TimelineRangeQuery,
        ) -> PortFuture<'_, crate::Result<crate::TimelineRangeSlice>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("timeline selected range failed").unwrap(),
                ))
            } else {
                let kinds: Vec<_> = query.kind_names();
                let matched: Vec<_> = self
                    .observations
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|observation| {
                        observation.session_id() == query.session_id
                            && observation.target_id() == query.target_id
                            && query.range.contains(observation.session_time())
                            && kinds.contains(&observation.kind().as_str())
                    })
                    .cloned()
                    .collect();
                let matched_count = matched.len() as u64;
                let limit = usize::from(query.limit.get());
                let truncated = matched_count as usize > limit;
                let observations = if truncated {
                    matched.into_iter().take(limit).collect()
                } else {
                    matched
                };
                Ok(crate::TimelineRangeSlice {
                    matched_count,
                    observations,
                    truncated,
                })
            };
            Box::pin(async move { result })
        }
    }

    fn browser_session(fail: bool, profile: ProfileRef) -> Arc<dyn BrowserSessionPort> {
        let target = PageTarget::new(
            TargetId::from_uuid(UUID.parse().unwrap()),
            "page-1",
            "https://example.test",
            "Example",
        )
        .unwrap();
        Arc::new(FakeBrowserSession {
            compatibility: BrowserCompatibility::new(
                BrowserVersion::new(
                    BrowserProduct::Chrome,
                    BrowserProductVersion::new("128").unwrap(),
                    "revision",
                    "1.3",
                    "Chrome/128",
                    "12",
                )
                .unwrap(),
                RendererCapability::ALL
                    .iter()
                    .copied()
                    .map(|capability| {
                        crate::CapabilitySupport::new(capability, true, true, None).unwrap()
                    })
                    .collect(),
            )
            .unwrap(),
            profile,
            session_id: SessionId::from_uuid(UUID.parse().unwrap()),
            session_origin: SessionOrigin::new(crate::ObservedTime::from_nanos(0)),
            targets: vec![SupervisedTarget {
                target,
                lifecycle: TargetLifecycle::Discovered,
                visibility: TargetVisibility::Unknown,
                attachment_generation: 0,
            }],
            fail,
        })
    }

    fn metadata() -> CapturedFrame {
        CapturedFrame::new(
            crate::FrameId::from_uuid(UUID.parse().unwrap()),
            SessionId::from_uuid(UUID.parse().unwrap()),
            TargetId::from_uuid(UUID.parse().unwrap()),
            crate::CaptureOrdinal::new(1).unwrap(),
            Some(SourceTime::from_nanos(2)),
            ObservedTime::from_nanos(3),
            SessionTime::from_nanos(1),
            ImageFormat::Jpeg,
            PixelDimensions::new(2, 2).unwrap(),
            PixelDimensions::new(2, 2).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap()
    }

    fn observation() -> TimelineObservation {
        TimelineObservation::new(
            SessionId::from_uuid(UUID.parse().unwrap()),
            TargetId::from_uuid(UUID.parse().unwrap()),
            SessionTime::from_nanos(1),
            None,
            ObservedTime::from_nanos(2),
            ObservationKind::Marker,
            ObservationPayloadRef::Marker(crate::MarkerId::from_uuid(UUID.parse().unwrap())),
        )
        .unwrap()
    }

    #[test]
    fn fake_clock_and_id_ports_are_deterministic() {
        let clocks: Arc<dyn MonotonicClock> = Arc::new(FakeClocks);
        let wall: Arc<dyn WallClock> = Arc::new(FakeClocks);
        assert_eq!(clocks.now().as_nanos(), 42);
        assert_eq!(wall.now(), SystemTime::UNIX_EPOCH + Duration::from_secs(7));

        let id = crate::IdValue::from_uuid(UUID.parse().unwrap());
        let ids: Arc<dyn IdSource> = Arc::new(FakeIds {
            values: Mutex::new(VecDeque::from([id])),
        });
        assert_eq!(ids.next(), id);
    }

    #[test]
    fn launch_defaults_to_the_named_reusable_managed_profile() {
        let request = LaunchBrowser::default();
        assert!(request.executable.is_none());
        assert!(request.initial_url.is_none());
        assert_eq!(request.every_nth_frame, EveryNthFrame::default());
        assert_eq!(
            request.profile,
            ManagedProfile::Reusable {
                name: ProfileIdentity::new(crate::DEFAULT_MANAGED_PROFILE_NAME).unwrap(),
            }
        );
    }

    #[test]
    fn every_nth_frame_accepts_only_the_inclusive_integer_bounds() {
        for value in [MIN_EVERY_NTH_FRAME, MAX_EVERY_NTH_FRAME] {
            let stride = EveryNthFrame::new(value).unwrap();
            assert_eq!(stride.get(), value);
            assert_eq!(
                serde_json::to_value(stride).unwrap(),
                serde_json::json!(value)
            );
            assert_eq!(
                serde_json::from_value::<EveryNthFrame>(serde_json::json!(value)).unwrap(),
                stride
            );
        }
        for value in [0, 61, u8::MAX] {
            assert!(EveryNthFrame::new(value).is_err());
        }
        for value in [
            serde_json::json!(0),
            serde_json::json!(61),
            serde_json::json!(null),
            serde_json::json!("1"),
            serde_json::json!(1.5),
        ] {
            assert!(serde_json::from_value::<EveryNthFrame>(value).is_err());
        }
    }

    #[test]
    fn launch_and_attach_stride_contracts_default_and_publish_generated_bounds() {
        let launch: LaunchBrowser = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(launch.every_nth_frame, EveryNthFrame::default());
        let launch_stride = EveryNthFrame::new(17).unwrap();
        let launch = LaunchBrowser {
            every_nth_frame: launch_stride,
            ..LaunchBrowser::default()
        };
        let launch_json = serde_json::to_value(&launch).unwrap();
        assert_eq!(launch_json["every_nth_frame"], serde_json::json!(17));
        assert_eq!(
            serde_json::from_value::<LaunchBrowser>(launch_json).unwrap(),
            launch
        );
        assert!(
            serde_json::from_value::<LaunchBrowser>(serde_json::json!({
                "every_nth_frame": 0
            }))
            .is_err()
        );

        let attach: AttachBrowser = serde_json::from_value(serde_json::json!({
            "endpoint": "ws://localhost:9222"
        }))
        .unwrap();
        assert_eq!(attach.every_nth_frame, EveryNthFrame::default());
        let attach = AttachBrowser::new("ws://localhost:9222")
            .unwrap()
            .with_every_nth_frame(EveryNthFrame::new(60).unwrap());
        let attach_json = serde_json::to_value(&attach).unwrap();
        assert_eq!(attach_json["every_nth_frame"], serde_json::json!(60));
        assert_eq!(
            serde_json::from_value::<AttachBrowser>(attach_json).unwrap(),
            attach
        );
        assert!(
            serde_json::from_value::<AttachBrowser>(serde_json::json!({
                "endpoint": "ws://localhost:9222",
                "every_nth_frame": 61
            }))
            .is_err()
        );

        for schema in [
            serde_json::to_value(schemars::schema_for!(EveryNthFrame)).unwrap(),
            serde_json::to_value(schemars::schema_for!(LaunchBrowser)).unwrap(),
            serde_json::to_value(schemars::schema_for!(AttachBrowser)).unwrap(),
        ] {
            let stride_schema = if schema["type"] == "integer" {
                schema
            } else {
                schema["properties"]["every_nth_frame"].clone()
            };
            assert_eq!(stride_schema["type"], "integer");
            assert_eq!(stride_schema["minimum"], serde_json::json!(1));
            assert_eq!(stride_schema["maximum"], serde_json::json!(60));
            assert_eq!(stride_schema["default"], serde_json::json!(1));
        }
        for schema in [
            serde_json::to_value(schemars::schema_for!(LaunchBrowser)).unwrap(),
            serde_json::to_value(schemars::schema_for!(AttachBrowser)).unwrap(),
        ] {
            let required = schema["required"].as_array();
            assert!(
                !required.is_some_and(|fields| {
                    fields.iter().any(|field| field == "every_nth_frame")
                })
            );
        }
    }

    #[test]
    fn browser_failure_kind_maps_exhaustively_to_safe_core_errors() {
        assert_eq!(
            BrowserFailureKind::ALL.len(),
            ErrorCode::BROWSER_SESSION_CODES.len()
        );
        for (kind, code) in BrowserFailureKind::ALL
            .iter()
            .zip(ErrorCode::BROWSER_SESSION_CODES)
        {
            assert_eq!(kind.error_code(), *code);
            let error = kind.into_error(crate::NonEmptyText::new("adapter failure").unwrap());
            assert_eq!(error.code, *code);
            assert!(error.recovery.is_some());
        }
    }

    #[test]
    fn browser_port_supports_object_safe_lifecycle_and_event_closure() {
        let managed_profile = ProfileRef::managed(ProfileIdentity::new("profile").unwrap());
        let connector: Arc<dyn BrowserConnector> = Arc::new(FakeBrowserConnector {
            session: browser_session(false, managed_profile.clone()),
            fail: false,
        });
        assert!(block_on(connector.installations()).unwrap().is_empty());
        let session = block_on(connector.connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("local").unwrap(),
        )))
        .unwrap();
        let status = block_on(session.status()).unwrap();
        assert_eq!(
            status.compatibility.version.product(),
            BrowserProduct::Chrome
        );
        assert_eq!(status.profile, managed_profile);
        assert_eq!(status.pages.len(), 1);
        let mut events = block_on(session.subscribe()).unwrap();
        assert!(block_on(events.next()).unwrap().is_none());
        let geometry_error = block_on(CurrentReferenceGeometry::current_reference_geometry(
            session.as_ref(),
            CurrentReferenceGeometryRequest::new(
                status.session_id,
                crate::NodeReference {
                    target_id: status.pages[0].target.target.id(),
                    generation: SnapshotGeneration::new(1).unwrap(),
                    node_id: SnapshotNodeId::new(1).unwrap(),
                },
            )
            .unwrap(),
        ))
        .unwrap_err();
        assert_eq!(geometry_error.code, ErrorCode::InvalidLifecycleTransition);
        assert_eq!(
            block_on(session.stop()).unwrap(),
            BrowserStopOutcome::ManagedBrowserClosed
        );

        let external = ProfileRef::External;
        let attached: Arc<dyn BrowserConnector> = Arc::new(FakeBrowserConnector {
            session: browser_session(false, external),
            fail: false,
        });
        let session = block_on(attached.connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("local").unwrap(),
        )))
        .unwrap();
        assert_eq!(
            block_on(session.stop()).unwrap(),
            BrowserStopOutcome::Detached
        );

        let failing: Arc<dyn BrowserConnector> = Arc::new(FakeBrowserConnector {
            session: browser_session(true, ProfileRef::External),
            fail: true,
        });
        let result = block_on(
            failing.connect(BrowserConnectRequest::Launch(LaunchBrowser {
                executable: None,
                profile: ManagedProfile::Temporary,
                initial_url: None,
                every_nth_frame: EveryNthFrame::default(),
                focus: BrowserFocusPolicy::default(),
            })),
        );
        assert_eq!(result.err().unwrap().code, ErrorCode::BrowserNotFound);
    }

    #[test]
    fn recording_port_separates_frames_gaps_and_flush() {
        let sink: Arc<dyn RecordingSink> = Arc::new(FakeRecording::default());
        let frame = EncodedFrame::new(metadata(), vec![1, 2, 3]).unwrap();
        let session_id = metadata().session_id();
        let target_id = metadata().target_id();
        let gap = CaptureGap::new(
            crate::GapId::from_uuid(UUID.parse().unwrap()),
            session_id,
            target_id,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
            ObservedTime::from_nanos(1),
            CaptureGapReason::CaptureStopped,
            NonZeroU64::new(1),
            None,
        )
        .unwrap();
        let address = block_on(sink.append_frame(frame)).unwrap();
        assert_eq!(
            address.segment_id,
            SegmentId::from_uuid(UUID.parse().unwrap())
        );
        assert_eq!(address.byte_offset.get(), 1);
        assert!(block_on(sink.append_gap(gap)).is_ok());
        assert!(block_on(sink.flush(session_id)).is_ok());

        let failing: Arc<dyn RecordingSink> = Arc::new(FakeRecording {
            fail: true,
            ..Default::default()
        });
        assert_eq!(
            block_on(failing.flush(session_id)).unwrap_err().code,
            ErrorCode::PersistenceFailed
        );
    }

    #[test]
    fn timeline_port_indexes_and_filters_ranges() {
        let store: Arc<dyn TimelineStore> = Arc::new(FakeTimeline::default());
        let item = observation();
        let session_id = item.session_id();
        let target_id = item.target_id();
        assert!(block_on(store.append(item)).is_ok());
        let result = block_on(store.range(
            session_id,
            target_id,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
        ))
        .unwrap();
        assert_eq!(result.len(), 1);

        let failing: Arc<dyn TimelineStore> = Arc::new(FakeTimeline {
            fail: true,
            ..Default::default()
        });
        assert_eq!(
            block_on(failing.range(
                session_id,
                target_id,
                SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
            ))
            .unwrap_err()
            .code,
            ErrorCode::PersistenceFailed
        );
    }

    #[test]
    fn core_ports_have_no_runtime_or_transport_types() {
        let sources = [
            include_str!("mod.rs"),
            include_str!("artifacts.rs"),
            include_str!("browser.rs"),
            include_str!("browser_events.rs"),
            include_str!("catalog.rs"),
            include_str!("clock.rs"),
            include_str!("frames.rs"),
            include_str!("gaps.rs"),
            include_str!("ids.rs"),
            include_str!("progressive.rs"),
            include_str!("recording.rs"),
            include_str!("retention.rs"),
            include_str!("timeline.rs"),
            include_str!("range.rs"),
            include_str!("video.rs"),
        ];
        for source in sources {
            for forbidden in [
                ["to", "kio"].concat(),
                ["async", "_trait"].concat(),
                ["Web", "Socket"].concat(),
                ["web", "socket"].concat(),
                ["sql", "ite"].concat(),
                ["SQL", "ite"].concat(),
                ["c", "dp"].concat(),
            ] {
                assert!(
                    !source.contains(&forbidden),
                    "found forbidden type marker {forbidden}"
                );
            }
        }
        let manifest = include_str!("../../Cargo.toml");
        for forbidden in [["to", "kio"].concat(), ["async", "_trait"].concat()] {
            assert!(!manifest.contains(&forbidden));
        }
    }
}
