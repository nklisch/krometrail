//! Runtime-neutral infrastructure ports.
//!
//! Implementations belong to sibling adapter crates. This module contains only
//! domain-owned values and `std` traits so the core remains usable with any
//! executor or transport implementation.

pub mod browser;
pub mod clock;
pub mod ids;
pub mod recording;
pub mod timeline;

/// The allocation is paid at an infrastructure boundary, keeping core traits
/// object-safe without selecting an async runtime or procedural macro.
pub type PortFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub use browser::{
    AttachBrowser, BrowserCompatibility, BrowserConnectRequest, BrowserConnector,
    BrowserSessionPort, DomainSupport, LaunchBrowser,
};
pub use clock::{MonotonicClock, WallClock};
pub use ids::IdSource;
pub use recording::RecordingSink;
pub use timeline::TimelineStore;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserVersion, CaptureGap, CaptureGapReason, CapturedFrame, DeviceScaleFactor,
        EncodedFrame, ErrorCode, ImageFormat, ObservationKind, ObservationPayloadRef, ObservedTime,
        PageTarget, PixelDimensions, ProfileIdentity, SessionId, SessionRange, SessionTime,
        SourceTime, TargetId, TimelineObservation,
    };
    use std::{
        collections::VecDeque,
        num::NonZeroU64,
        pin::Pin,
        sync::{Arc, Mutex},
        task::{Context, Poll, Wake, Waker},
        time::{Duration, SystemTime},
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    struct NoopWaker;

    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}

        fn wake_by_ref(self: &Arc<Self>) {}
    }

    fn block_on<T>(future: PortFuture<'_, T>) -> T {
        let waker = Waker::from(Arc::new(NoopWaker));
        let mut context = Context::from_waker(&waker);
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
    struct FakeBrowserSession {
        compatibility: BrowserCompatibility,
        targets: Vec<PageTarget>,
        fail: bool,
    }

    impl BrowserSessionPort for FakeBrowserSession {
        fn compatibility(&self) -> &BrowserCompatibility {
            &self.compatibility
        }

        fn page_targets(&self) -> PortFuture<'_, crate::Result<Vec<PageTarget>>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::BrowserDisconnected,
                    crate::NonEmptyText::new("browser session is unavailable").unwrap(),
                ))
            } else {
                Ok(self.targets.clone())
            };
            Box::pin(async move { result })
        }

        fn close(&self) -> PortFuture<'_, crate::Result<()>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::BrowserDisconnected,
                    crate::NonEmptyText::new("browser session is unavailable").unwrap(),
                ))
            } else {
                Ok(())
            };
            Box::pin(async move { result })
        }
    }

    struct FakeBrowserConnector {
        session: Arc<dyn BrowserSessionPort>,
        fail: bool,
    }

    impl BrowserConnector for FakeBrowserConnector {
        fn connect(
            &self,
            _request: BrowserConnectRequest,
        ) -> PortFuture<'_, crate::Result<Arc<dyn BrowserSessionPort>>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::BrowserDisconnected,
                    crate::NonEmptyText::new("browser connection is unavailable").unwrap(),
                ))
            } else {
                Ok(Arc::clone(&self.session))
            };
            Box::pin(async move { result })
        }
    }

    #[derive(Debug, Default)]
    struct FakeRecording {
        frames: Mutex<Vec<EncodedFrame>>,
        gaps: Mutex<Vec<CaptureGap>>,
        flushes: Mutex<Vec<SessionId>>,
        fail: bool,
    }

    impl RecordingSink for FakeRecording {
        fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, crate::Result<()>> {
            let result = if self.fail {
                Err(crate::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    crate::NonEmptyText::new("frame persistence failed").unwrap(),
                ))
            } else {
                self.frames.lock().unwrap().push(frame);
                Ok(())
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
    }

    fn browser_session(fail: bool) -> Arc<dyn BrowserSessionPort> {
        Arc::new(FakeBrowserSession {
            compatibility: BrowserCompatibility {
                version: BrowserVersion::new("Chrome", "revision", "1").unwrap(),
                required_domains: vec![DomainSupport {
                    domain: "page".into(),
                    available: true,
                    detail: None,
                }],
            },
            targets: vec![
                PageTarget::new(
                    TargetId::from_uuid(UUID.parse().unwrap()),
                    "page-1",
                    "https://example.test",
                    "Example",
                )
                .unwrap(),
            ],
            fail,
        })
    }

    fn metadata() -> CapturedFrame {
        CapturedFrame::new(
            crate::FrameId::from_uuid(UUID.parse().unwrap()),
            SessionId::from_uuid(UUID.parse().unwrap()),
            TargetId::from_uuid(UUID.parse().unwrap()),
            1,
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
    fn browser_port_supports_object_safe_success_and_failure() {
        let connector: Arc<dyn BrowserConnector> = Arc::new(FakeBrowserConnector {
            session: browser_session(false),
            fail: false,
        });
        let session = block_on(
            connector.connect(BrowserConnectRequest::Attach(AttachBrowser {
                endpoint: "local".into(),
            })),
        )
        .unwrap();
        assert_eq!(session.compatibility().version.product(), "Chrome");
        assert_eq!(block_on(session.page_targets()).unwrap().len(), 1);
        assert!(block_on(session.close()).is_ok());

        let failing: Arc<dyn BrowserConnector> = Arc::new(FakeBrowserConnector {
            session: browser_session(true),
            fail: true,
        });
        let result = block_on(
            failing.connect(BrowserConnectRequest::Launch(LaunchBrowser {
                profile: ProfileIdentity::new("profile").unwrap(),
                initial_url: None,
            })),
        );
        assert_eq!(result.err().unwrap().code, ErrorCode::BrowserDisconnected);
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
            CaptureGapReason::CaptureStopped,
            NonZeroU64::new(1),
            None,
        )
        .unwrap();
        assert!(block_on(sink.append_frame(frame)).is_ok());
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
            include_str!("browser.rs"),
            include_str!("clock.rs"),
            include_str!("ids.rs"),
            include_str!("recording.rs"),
            include_str!("timeline.rs"),
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
