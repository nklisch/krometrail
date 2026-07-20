use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ByteOffset, CaptureGapReason, CaptureStreamState, DiskBudgetBytes, EncodedFrame, ErrorCode,
    FrameAddress, IdValue, ImageFormat, KrometrailError, MonotonicClock, NonEmptyText,
    ObservedTime, PersistenceFailure, PersistenceFailureCategory, PersistenceOperation,
    PersistenceRecoverability, PinChange, PortFuture, RecordingSink, RetentionRange,
    RetentionStatus, RetentionStore, SegmentId, SessionDeletion, SessionId, SessionOrigin,
    SessionTime, TargetId,
};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::sync::{Notify, mpsc, watch};

use crate::transport::{
    CdpTransport, CommandScope, NamedEvent, TransportClose, TransportError, TransportEvents,
    TransportFuture, TransportSessionId,
};

#[test]
fn default_configuration_matches_bounded_capture_contract() {
    let config = CaptureConfig::default();
    assert!(config.validate().is_ok());
    assert_eq!(config.max_active_streams.get(), 8);
    assert_eq!(config.queue_capacity.get(), 4);
    assert_eq!(config.max_base64_payload_bytes.get(), 8 * 1024 * 1024);
    assert_eq!(config.ack_timeout, std::time::Duration::from_secs(1));
    assert_eq!(CaptureConfig::max_queued_payload_bytes(), 256 * 1024 * 1024);
}

#[test]
fn configuration_rejects_hard_caps_and_aggregate_over_budget() {
    let config = CaptureConfig {
        max_active_streams: NonZeroUsize::new(33).unwrap(),
        ..CaptureConfig::default()
    };
    assert!(config.validate().is_err());
    let config = CaptureConfig {
        max_active_streams: NonZeroUsize::new(32).unwrap(),
        queue_capacity: NonZeroUsize::new(16).unwrap(),
        max_base64_payload_bytes: NonZeroUsize::new(16 * 1024 * 1024).unwrap(),
        ..CaptureConfig::default()
    };
    assert!(config.validate().is_err());
    let maximum_valid = CaptureConfig {
        max_active_streams: NonZeroUsize::new(32).unwrap(),
        queue_capacity: NonZeroUsize::new(1).unwrap(),
        max_base64_payload_bytes: NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
        ..CaptureConfig::default()
    };
    assert!(maximum_valid.validate().is_ok());
}

#[test]
fn transition_table_keeps_terminals_terminal() {
    use krometrail_core::CaptureStreamState::*;
    use pipeline::{Transition, next_state};
    assert_eq!(
        next_state(Starting, Transition::StartedVisible),
        Some(Capturing)
    );
    assert_eq!(next_state(Capturing, Transition::Hide), Some(Hidden));
    assert_eq!(
        next_state(Hidden, Transition::PauseBudget),
        Some(PausedBudget)
    );
    assert_eq!(
        next_state(PausedBudget, Transition::ResumeBudgetHidden),
        Some(Hidden)
    );
    assert_eq!(next_state(Hidden, Transition::ActualFrame), Some(Capturing));
    assert_eq!(next_state(Capturing, Transition::Suspend), Some(Suspended));
    assert_eq!(next_state(Suspended, Transition::Resume), Some(Starting));
    assert_eq!(next_state(Capturing, Transition::Stop), Some(Draining));
    assert_eq!(next_state(Draining, Transition::Deadline), Some(Stopped));
    assert_eq!(next_state(Stopped, Transition::StartedVisible), None);
    assert_eq!(next_state(Failed, Transition::Resume), None);
}

#[test]
fn logarithmic_histogram_is_fixed_size_and_nearest_rank_is_deterministic() {
    let mut histogram = pipeline::Histogram::default();
    for value in [0, 1, 2, 3, 4, 8, 16, 32] {
        histogram.record(value);
    }
    let summary = histogram.summary();
    assert_eq!(summary.sample_count(), 8);
    assert_eq!(summary.p50_nanos(), Some(3));
    assert_eq!(summary.p95_nanos(), Some(63));
    assert_eq!(summary.p99_nanos(), Some(63));
    assert_eq!(summary.max_nanos(), Some(32));
    assert_eq!(
        std::mem::size_of_val(&histogram.buckets),
        64 * std::mem::size_of::<u64>()
    );
}

#[test]
fn stable_capture_names_and_gap_reasons_are_registry_backed() {
    assert_eq!(CaptureStreamState::ALL.len(), 8);
    assert_eq!(
        CaptureGapReason::ALL.last().unwrap().as_str(),
        "frame_rejected"
    );
    for state in CaptureStreamState::ALL {
        let encoded = serde_json::to_string(state).unwrap();
        assert_eq!(
            serde_json::from_str::<CaptureStreamState>(&encoded).unwrap(),
            *state
        );
    }
}

#[test]
fn every_capture_boundary_stage_is_stable_and_first_failure_wins() {
    use krometrail_core::{
        CaptureFailure, CaptureFailureStage, ErrorCode, KrometrailError, NonEmptyText,
    };

    let failure = |stage| {
        CaptureFailure::new(
            stage,
            KrometrailError::new(
                ErrorCode::CaptureFailed,
                NonEmptyText::new("capture stage failed").unwrap(),
            ),
        )
        .unwrap()
    };

    for stage in CaptureFailureStage::ALL {
        let mut current = None;
        let expected = failure(*stage);
        assert!(pipeline::record_first_failure(
            &mut current,
            expected.clone()
        ));
        assert_eq!(current, Some(expected.clone()));
        assert!(!pipeline::record_first_failure(
            &mut current,
            failure(CaptureFailureStage::GapPersistence)
        ));
        assert_eq!(current, Some(expected));
    }

    let source = include_str!("pipeline.rs");
    for stage in [
        "FrameEventStream",
        "VisibilityEventStream",
        "Acknowledgement",
        "FrameEnvelope",
        "FrameDecode",
        "FramePersistence",
        "GapPersistence",
    ] {
        assert!(
            source.contains(&format!("CaptureFailureStage::{stage}")),
            "capture boundary {stage} must retain an explicit failure stage"
        );
    }
}

#[test]
fn image_header_parser_is_local_and_bounded() {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13_u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&4_u32.to_be_bytes());
    png.extend_from_slice(&3_u32.to_be_bytes());
    assert_eq!(
        image_header::dimensions(ImageFormat::Png, &png)
            .unwrap()
            .height(),
        3
    );
    assert!(image_header::dimensions(ImageFormat::Jpeg, b"bad").is_err());
}

#[derive(Debug)]
struct TestClock {
    next: AtomicU64,
    stride: u64,
    calls: AtomicU64,
}

impl TestClock {
    fn new() -> Self {
        Self::with_stride(1)
    }

    fn with_stride(stride: u64) -> Self {
        Self {
            next: AtomicU64::new(1),
            stride,
            calls: AtomicU64::new(0),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }
}

impl MonotonicClock for TestClock {
    fn now(&self) -> ObservedTime {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ObservedTime::from_nanos(self.next.fetch_add(self.stride, Ordering::Relaxed))
    }
}

#[derive(Debug)]
struct TestIds {
    next: AtomicU64,
}

impl TestIds {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }
}

impl krometrail_core::IdSource for TestIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(uuid::Uuid::from_u128(
            self.next.fetch_add(1, Ordering::Relaxed) as u128,
        ))
    }
}

#[derive(Debug, Default)]
struct TestObserver {
    statuses: Mutex<Vec<krometrail_core::TargetCaptureStatus>>,
    gaps: Mutex<Vec<krometrail_core::CaptureGap>>,
    visibility: Mutex<Vec<krometrail_core::TargetVisibility>>,
    visibility_times: Mutex<Vec<SessionTime>>,
    geometry_refreshes: Mutex<Vec<CaptureGeometryTransition>>,
    defer_geometry_refresh: AtomicBool,
    capture_failures: Mutex<Vec<u64>>,
}

impl CaptureObserver for TestObserver {
    fn status_changed(&self, status: krometrail_core::TargetCaptureStatus) {
        self.statuses.lock().unwrap().push(status);
    }

    fn gap_declared(&self, gap: krometrail_core::CaptureGap) {
        self.gaps.lock().unwrap().push(gap);
    }

    fn capture_stream_failed(&self, connection_generation: u64) {
        self.capture_failures
            .lock()
            .unwrap()
            .push(connection_generation);
    }

    fn visibility_changed(
        &self,
        _target_id: TargetId,
        visibility: krometrail_core::TargetVisibility,
        observed_at: SessionTime,
    ) {
        self.visibility.lock().unwrap().push(visibility);
        self.visibility_times.lock().unwrap().push(observed_at);
    }

    fn geometry_refresh_requested(&self, transition: CaptureGeometryTransition) -> bool {
        self.geometry_refreshes.lock().unwrap().push(transition);
        !self.defer_geometry_refresh.load(Ordering::Acquire)
    }
}

impl TestObserver {
    async fn wait_for_geometry_refreshes(&self, count: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while self.geometry_refreshes.lock().unwrap().len() < count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[derive(Debug, Default)]
struct TestRetention {
    allowed: AtomicBool,
    changed: Notify,
}

impl TestRetention {
    fn available() -> Self {
        Self {
            allowed: AtomicBool::new(true),
            changed: Notify::new(),
        }
    }

    fn pause(&self) {
        self.allowed.store(false, Ordering::Release);
    }

    fn resume(&self) {
        self.allowed.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
}

impl RetentionStore for TestRetention {
    fn pin_range(
        &self,
        request: RetentionRange,
    ) -> PortFuture<'_, krometrail_core::Result<PinChange>> {
        Box::pin(std::future::ready(Ok(PinChange {
            request,
            protected_segments: Vec::new(),
            pinned_usage_bytes: 0,
        })))
    }

    fn unpin_range(
        &self,
        request: RetentionRange,
    ) -> PortFuture<'_, krometrail_core::Result<PinChange>> {
        self.pin_range(request)
    }

    fn enforce_budget(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        self.status()
    }

    fn status(&self) -> PortFuture<'_, krometrail_core::Result<RetentionStatus>> {
        Box::pin(std::future::ready(Ok(RetentionStatus::empty(
            DiskBudgetBytes::default(),
        ))))
    }

    fn delete_session(
        &self,
        session_id: SessionId,
    ) -> PortFuture<'_, krometrail_core::Result<SessionDeletion>> {
        Box::pin(std::future::ready(Ok(SessionDeletion {
            session_id,
            removed_segments: 0,
            removed_frames: 0,
            removed_artifacts: 0,
            removed_bytes: 0,
        })))
    }

    fn wait_until_recording_allowed(&self) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            while !self.allowed.load(Ordering::Acquire) {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.allowed.load(Ordering::Acquire) {
                    break;
                }
                changed.await;
            }
            Ok(())
        })
    }
}

#[derive(Debug)]
struct TestSink {
    frames: Mutex<Vec<EncodedFrame>>,
    gaps: Mutex<Vec<krometrail_core::CaptureGap>>,
    flushes: AtomicU64,
    first_frame_started: Notify,
    release_first_frame: Notify,
    frame_calls: AtomicU64,
    ack_completed: Arc<AtomicBool>,
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl TestSink {
    fn new(ack_completed: Arc<AtomicBool>, order: Arc<Mutex<Vec<&'static str>>>) -> Self {
        Self {
            frames: Mutex::new(Vec::new()),
            gaps: Mutex::new(Vec::new()),
            flushes: AtomicU64::new(0),
            first_frame_started: Notify::new(),
            release_first_frame: Notify::new(),
            frame_calls: AtomicU64::new(0),
            ack_completed,
            order,
        }
    }
}

impl RecordingSink for TestSink {
    fn append_frame(
        &self,
        frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        assert!(self.ack_completed.load(Ordering::Acquire));
        self.order.lock().unwrap().push("sink");
        self.frames.lock().unwrap().push(frame);
        let call = self.frame_calls.fetch_add(1, Ordering::AcqRel);
        let address = FrameAddress::new(
            SegmentId::from_uuid(uuid::Uuid::from_u128(1)),
            ByteOffset::new(call + 1),
        );
        if call == 0 {
            self.first_frame_started.notify_one();
            Box::pin(async move {
                self.release_first_frame.notified().await;
                Ok(address)
            })
        } else {
            Box::pin(std::future::ready(Ok(address)))
        }
    }

    fn append_gap(
        &self,
        gap: krometrail_core::CaptureGap,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.gaps.lock().unwrap().push(gap);
        Box::pin(std::future::ready(Ok(())))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.flushes.fetch_add(1, Ordering::Relaxed);
        Box::pin(std::future::ready(Ok(())))
    }
}

#[derive(Debug)]
struct BudgetSink {
    blocked: AtomicBool,
    frames: AtomicU64,
    gaps: Mutex<Vec<krometrail_core::CaptureGap>>,
}

impl BudgetSink {
    fn new_blocked() -> Self {
        Self {
            blocked: AtomicBool::new(true),
            frames: AtomicU64::new(0),
            gaps: Mutex::new(Vec::new()),
        }
    }

    fn resume(&self) {
        self.blocked.store(false, Ordering::Release);
    }
}

impl RecordingSink for BudgetSink {
    fn append_frame(
        &self,
        _frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        if self.blocked.load(Ordering::Acquire) {
            return Box::pin(std::future::ready(Err(KrometrailError::new(
                ErrorCode::BudgetExhausted,
                NonEmptyText::new("disk budget paused capture").unwrap(),
            ))));
        }
        let position = self.frames.fetch_add(1, Ordering::AcqRel) + 1;
        Box::pin(std::future::ready(Ok(FrameAddress::new(
            SegmentId::from_uuid(uuid::Uuid::from_u128(99)),
            ByteOffset::new(position),
        ))))
    }

    fn append_gap(
        &self,
        gap: krometrail_core::CaptureGap,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.gaps.lock().unwrap().push(gap);
        Box::pin(std::future::ready(Ok(())))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }
}

#[derive(Debug)]
struct RejectingSink {
    frame_error: KrometrailError,
    gap_error: Option<KrometrailError>,
    gaps: Mutex<Vec<krometrail_core::CaptureGap>>,
}

impl RecordingSink for RejectingSink {
    fn append_frame(
        &self,
        _frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        Box::pin(std::future::ready(Err(self.frame_error.clone())))
    }

    fn append_gap(
        &self,
        gap: krometrail_core::CaptureGap,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.gaps.lock().unwrap().push(gap);
        Box::pin(std::future::ready(match &self.gap_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }
}

#[tokio::test]
async fn classified_persistence_rejection_survives_as_first_capture_failure() {
    let frame_cause = KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new("sealed segment publication sync failed").unwrap(),
    )
    .with_persistence(PersistenceFailure::new(
        PersistenceOperation::SealedSegmentPublicationSync,
        PersistenceFailureCategory::PermissionDenied,
        PersistenceRecoverability::WriterUsable,
    ));
    let later_gap_cause = KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new("gap index failed").unwrap(),
    )
    .with_persistence(PersistenceFailure::new(
        PersistenceOperation::GapIndex,
        PersistenceFailureCategory::Unavailable,
        PersistenceRecoverability::WriterTerminal,
    ));
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport = TestTransport::new(ack_completed, Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(RejectingSink {
        frame_error: frame_cause.clone(),
        gap_error: Some(later_gap_cause),
        gaps: Mutex::new(Vec::new()),
    });
    let observer = Arc::new(TestObserver::default());
    let coordinator = CaptureCoordinator::new(
        CaptureConfig::default(),
        krometrail_core::EveryNthFrame::default(),
        CaptureDependencies {
            clock: Arc::new(TestClock::new()),
            ids: Arc::new(TestIds::new()),
            sink: Arc::clone(&sink) as Arc<dyn RecordingSink>,
            retention: Arc::new(TestRetention::available()),
        },
        Arc::clone(&observer) as Arc<dyn CaptureObserver>,
    )
    .unwrap();
    coordinator
        .start_target(target(), Arc::clone(&transport) as Arc<dyn CdpTransport>)
        .await
        .unwrap();
    transport.frame(1).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while coordinator.statuses()[0].state() != CaptureStreamState::Failed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let status = coordinator.statuses().remove(0);
    let failure = status.failure().expect("failed capture retains its cause");
    assert_eq!(
        failure.stage(),
        krometrail_core::CaptureFailureStage::FramePersistence
    );
    assert_eq!(failure.cause(), &frame_cause);
    assert_eq!(status.statistics().gap_count(), 1);
    assert_eq!(sink.gaps.lock().unwrap().len(), 1);
    assert_eq!(
        observer.gaps.lock().unwrap()[0].reason(),
        &CaptureGapReason::PersistenceRejected
    );
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("sealed_segment_publication_sync"));
    assert!(!json.contains("/private/recordings"));
    assert!(!json.contains("raw frame"));
    assert!(!json.contains("page content"));
}

#[derive(Debug)]
struct TestEvents {
    receiver: mpsc::Receiver<NamedEvent>,
}

impl TransportEvents for TestEvents {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>> {
        Box::pin(async { Ok(self.receiver.recv().await) })
    }
}

#[derive(Debug)]
struct TestTransport {
    default_session: TransportSessionId,
    frame_senders: Mutex<HashMap<TransportSessionId, mpsc::Sender<NamedEvent>>>,
    visibility_senders: Mutex<HashMap<TransportSessionId, mpsc::Sender<NamedEvent>>>,
    frame_receivers: Mutex<HashMap<TransportSessionId, mpsc::Receiver<NamedEvent>>>,
    visibility_receivers: Mutex<HashMap<TransportSessionId, mpsc::Receiver<NamedEvent>>>,
    geometry_senders: Mutex<HashMap<(TransportSessionId, String), mpsc::Sender<NamedEvent>>>,
    geometry_receivers: Mutex<HashMap<(TransportSessionId, String), mpsc::Receiver<NamedEvent>>>,
    calls: Mutex<Vec<String>>,
    start_params: Mutex<Vec<serde_json::Value>>,
    ack_watch: watch::Sender<usize>,
    ack_count: AtomicU64,
    ack_tokens: Mutex<Vec<i64>>,
    ack_completed: Arc<AtomicBool>,
    order: Arc<Mutex<Vec<&'static str>>>,
    fail_ack: AtomicBool,
    hold_ack: AtomicBool,
    ack_started: Arc<Notify>,
    release_ack: Arc<Notify>,
}

impl TestTransport {
    fn new(ack_completed: Arc<AtomicBool>, order: Arc<Mutex<Vec<&'static str>>>) -> Arc<Self> {
        let default_session = TransportSessionId::new("transport-session").unwrap();
        let (frame_sender, frame_receiver) = mpsc::channel(16);
        let (visibility_sender, visibility_receiver) = mpsc::channel(16);
        let mut frame_senders = HashMap::new();
        let mut frame_receivers = HashMap::new();
        frame_senders.insert(default_session.clone(), frame_sender);
        frame_receivers.insert(default_session.clone(), frame_receiver);
        let mut visibility_senders = HashMap::new();
        let mut visibility_receivers = HashMap::new();
        visibility_senders.insert(default_session.clone(), visibility_sender);
        visibility_receivers.insert(default_session.clone(), visibility_receiver);
        let mut geometry_senders = HashMap::new();
        let mut geometry_receivers = HashMap::new();
        for method in [
            "Page.frameResized",
            "Page.frameNavigated",
            "Page.navigatedWithinDocument",
        ] {
            let (sender, receiver) = mpsc::channel(16);
            geometry_senders.insert((default_session.clone(), method.to_owned()), sender);
            geometry_receivers.insert((default_session.clone(), method.to_owned()), receiver);
        }
        let (ack_watch, _) = watch::channel(0);
        Arc::new(Self {
            default_session,
            frame_senders: Mutex::new(frame_senders),
            visibility_senders: Mutex::new(visibility_senders),
            frame_receivers: Mutex::new(frame_receivers),
            visibility_receivers: Mutex::new(visibility_receivers),
            geometry_senders: Mutex::new(geometry_senders),
            geometry_receivers: Mutex::new(geometry_receivers),
            calls: Mutex::new(Vec::new()),
            start_params: Mutex::new(Vec::new()),
            ack_watch,
            ack_count: AtomicU64::new(0),
            ack_tokens: Mutex::new(Vec::new()),
            ack_completed,
            order,
            fail_ack: AtomicBool::new(false),
            hold_ack: AtomicBool::new(false),
            ack_started: Arc::new(Notify::new()),
            release_ack: Arc::new(Notify::new()),
        })
    }

    fn ensure_session(&self, session: &TransportSessionId) {
        {
            let senders = self.frame_senders.lock().unwrap();
            if senders.contains_key(session) {
                return;
            }
        }
        let (frame_sender, frame_receiver) = mpsc::channel(16);
        let (visibility_sender, visibility_receiver) = mpsc::channel(16);
        self.frame_senders
            .lock()
            .unwrap()
            .insert(session.clone(), frame_sender);
        self.frame_receivers
            .lock()
            .unwrap()
            .insert(session.clone(), frame_receiver);
        self.visibility_senders
            .lock()
            .unwrap()
            .insert(session.clone(), visibility_sender);
        self.visibility_receivers
            .lock()
            .unwrap()
            .insert(session.clone(), visibility_receiver);
        for method in [
            "Page.frameResized",
            "Page.frameNavigated",
            "Page.navigatedWithinDocument",
        ] {
            let (sender, receiver) = mpsc::channel(16);
            self.geometry_senders
                .lock()
                .unwrap()
                .insert((session.clone(), method.to_owned()), sender);
            self.geometry_receivers
                .lock()
                .unwrap()
                .insert((session.clone(), method.to_owned()), receiver);
        }
    }

    async fn frame(&self, ack_token: i64) {
        self.frame_for(&self.default_session, ack_token).await;
    }

    async fn frame_with_metadata(
        &self,
        ack_token: i64,
        viewport_width: u32,
        viewport_height: u32,
        timestamp: Option<f64>,
    ) {
        let sender = self
            .frame_senders
            .lock()
            .unwrap()
            .get(&self.default_session)
            .cloned()
            .expect("default frame session is registered");
        sender
            .send(NamedEvent {
                method: "Page.screencastFrame".into(),
                params: frame_params_with_metadata(
                    ack_token,
                    viewport_width,
                    viewport_height,
                    timestamp,
                ),
            })
            .await
            .unwrap();
    }

    async fn frame_with_encoding(
        &self,
        ack_token: i64,
        encoded_width: u16,
        encoded_height: u16,
        metadata_width: u32,
        metadata_height: u32,
        timestamp: Option<f64>,
    ) {
        let sender = self
            .frame_senders
            .lock()
            .unwrap()
            .get(&self.default_session)
            .cloned()
            .expect("default frame session is registered");
        sender
            .send(NamedEvent {
                method: "Page.screencastFrame".into(),
                params: frame_params_with_encoding(
                    ack_token,
                    encoded_width,
                    encoded_height,
                    metadata_width,
                    metadata_height,
                    timestamp,
                ),
            })
            .await
            .unwrap();
    }

    async fn frame_for(&self, session: &TransportSessionId, ack_token: i64) {
        let sender = self
            .frame_senders
            .lock()
            .unwrap()
            .get(session)
            .cloned()
            .unwrap_or_else(|| panic!("frame session {session:?} not registered"));
        sender
            .send(NamedEvent {
                method: "Page.screencastFrame".into(),
                params: frame_params(ack_token),
            })
            .await
            .unwrap();
    }

    async fn visibility(&self, visible: bool) {
        self.visibility_for(&self.default_session, visible).await;
    }

    async fn visibility_for(&self, session: &TransportSessionId, visible: bool) {
        let sender = self
            .visibility_senders
            .lock()
            .unwrap()
            .get(session)
            .cloned()
            .unwrap_or_else(|| panic!("visibility session {session:?} not registered"));
        sender
            .send(NamedEvent {
                method: "Page.screencastVisibilityChanged".into(),
                params: serde_json::json!({"visible": visible}),
            })
            .await
            .unwrap();
    }

    async fn geometry_event(&self, method: &str) {
        let sender = self
            .geometry_senders
            .lock()
            .unwrap()
            .get(&(self.default_session.clone(), method.to_owned()))
            .cloned()
            .expect("geometry event session is registered");
        sender
            .send(NamedEvent {
                method: method.to_owned(),
                params: serde_json::json!({}),
            })
            .await
            .unwrap();
    }

    fn start_params(&self) -> Vec<serde_json::Value> {
        self.start_params.lock().unwrap().clone()
    }

    async fn wait_for_acks(&self, count: usize) {
        let mut receiver = self.ack_watch.subscribe();
        while *receiver.borrow() < count {
            receiver.changed().await.unwrap();
        }
    }

    fn hold_ack(&self) {
        self.hold_ack.store(true, Ordering::Release);
    }

    async fn wait_for_ack_start(&self) {
        self.ack_started.notified().await;
    }

    fn release_ack(&self) {
        self.release_ack.notify_one();
    }
}

impl CdpTransport for TestTransport {
    fn send_raw(
        &self,
        _scope: &CommandScope,
        method: &str,
        params: serde_json::Value,
    ) -> TransportFuture<'_, Result<serde_json::Value, TransportError>> {
        self.calls.lock().unwrap().push(method.to_owned());
        if method == "Page.startScreencast" {
            self.start_params.lock().unwrap().push(params.clone());
        }
        if method == "Page.screencastFrameAck" {
            if let Some(token) = params.get("sessionId").and_then(serde_json::Value::as_i64) {
                self.ack_tokens.lock().unwrap().push(token);
            }
            if self.fail_ack.load(Ordering::Acquire) {
                return Box::pin(std::future::ready(Err(TransportError::CommandFailed)));
            }
            if self.hold_ack.swap(false, Ordering::AcqRel) {
                let started = Arc::clone(&self.ack_started);
                let release = Arc::clone(&self.release_ack);
                let order = Arc::clone(&self.order);
                let completed = Arc::clone(&self.ack_completed);
                let count = &self.ack_count;
                let watch = self.ack_watch.clone();
                return Box::pin(async move {
                    started.notify_one();
                    release.notified().await;
                    order.lock().unwrap().push("ack");
                    completed.store(true, Ordering::Release);
                    let count = count.fetch_add(1, Ordering::AcqRel) + 1;
                    let _ = watch.send(count as usize);
                    Ok(serde_json::json!({}))
                });
            }
            self.order.lock().unwrap().push("ack");
            self.ack_completed.store(true, Ordering::Release);
            let count = self.ack_count.fetch_add(1, Ordering::AcqRel) + 1;
            let _ = self.ack_watch.send(count as usize);
        }
        Box::pin(std::future::ready(Ok(serde_json::json!({}))))
    }

    fn subscribe_named(
        &self,
        scope: &CommandScope,
        method: &str,
    ) -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>> {
        let CommandScope::Session(session) = scope else {
            return Box::pin(async move { Err(TransportError::InvalidInput) });
        };
        let session = session.clone();
        let receiver = match method {
            "Page.screencastFrame" => self.frame_receivers.lock().unwrap().remove(&session),
            "Page.screencastVisibilityChanged" => {
                self.visibility_receivers.lock().unwrap().remove(&session)
            }
            "Page.frameResized" | "Page.frameNavigated" | "Page.navigatedWithinDocument" => self
                .geometry_receivers
                .lock()
                .unwrap()
                .remove(&(session, method.to_owned())),
            _ => None,
        };
        Box::pin(async move {
            receiver
                .map(|receiver| Box::new(TestEvents { receiver }) as Box<dyn TransportEvents>)
                .ok_or(TransportError::InvalidInput)
        })
    }

    fn close_reason(&self) -> Option<TransportClose> {
        None
    }

    fn is_closed(&self) -> bool {
        false
    }
}

fn frame_params(ack_token: i64) -> serde_json::Value {
    frame_params_with_metadata(ack_token, 640, 480, Some(1.25))
}

fn frame_params_with_metadata(
    ack_token: i64,
    viewport_width: u32,
    viewport_height: u32,
    timestamp: Option<f64>,
) -> serde_json::Value {
    frame_params_with_encoding(ack_token, 2, 2, viewport_width, viewport_height, timestamp)
}

fn frame_params_with_encoding(
    ack_token: i64,
    encoded_width: u16,
    encoded_height: u16,
    metadata_width: u32,
    metadata_height: u32,
    timestamp: Option<f64>,
) -> serde_json::Value {
    serde_json::json!({
        "data": STANDARD.encode(jpeg_bytes(encoded_width, encoded_height)),
        "metadata": {
            "deviceWidth": metadata_width,
            "deviceHeight": metadata_height,
            "pageScaleFactor": 1.0,
            "timestamp": timestamp
        },
        "sessionId": ack_token
    })
}

fn jpeg_bytes(width: u16, height: u16) -> Vec<u8> {
    let [height_high, height_low] = height.to_be_bytes();
    let [width_high, width_low] = width.to_be_bytes();
    vec![
        0xff,
        0xd8,
        0xff,
        0xc0,
        0,
        8,
        8,
        height_high,
        height_low,
        width_high,
        width_low,
        1,
        0xff,
        0xd9,
    ]
}

fn target() -> CaptureTarget {
    target_with(2, "transport-session", 1)
}

fn target_with(
    target_value: u128,
    transport_session: &str,
    attachment_generation: u64,
) -> CaptureTarget {
    CaptureTarget {
        session_id: SessionId::from_uuid(uuid::Uuid::from_u128(1)),
        session_origin: SessionOrigin::new(ObservedTime::from_nanos(0)),
        target_id: TargetId::from_uuid(uuid::Uuid::from_u128(target_value)),
        connection_generation: 1,
        attachment_generation,
        transport_session: TransportSessionId::new(transport_session).unwrap(),
        geometry: CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(600, 500).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(1.0).unwrap(),
        },
    }
}

fn coordinator_with_stride(
    config: CaptureConfig,
    every_nth_frame: krometrail_core::EveryNthFrame,
    clock: Arc<TestClock>,
    ids: Arc<TestIds>,
    sink: Arc<TestSink>,
    observer: Arc<TestObserver>,
) -> CaptureCoordinator {
    CaptureCoordinator::new(
        config,
        every_nth_frame,
        CaptureDependencies {
            clock,
            ids,
            sink,
            retention: Arc::new(TestRetention::available()),
        },
        observer,
    )
    .unwrap()
}

fn coordinator(
    config: CaptureConfig,
    clock: Arc<TestClock>,
    ids: Arc<TestIds>,
    sink: Arc<TestSink>,
    observer: Arc<TestObserver>,
) -> CaptureCoordinator {
    coordinator_with_stride(
        config,
        krometrail_core::EveryNthFrame::default(),
        clock,
        ids,
        sink,
        observer,
    )
}

#[tokio::test]
async fn start_screencast_forwards_the_immutable_stride_for_jpeg_and_png() {
    for (format, expected_format) in [(ImageFormat::Jpeg, "jpeg"), (ImageFormat::Png, "png")] {
        let ack_completed = Arc::new(AtomicBool::new(false));
        let transport =
            TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
        let sink = Arc::new(TestSink::new(
            ack_completed,
            Arc::new(Mutex::new(Vec::new())),
        ));
        let mut config = CaptureConfig {
            format,
            ..CaptureConfig::default()
        };
        if format == ImageFormat::Png {
            config.jpeg_quality = None;
        }
        let coordinator = coordinator_with_stride(
            config,
            krometrail_core::EveryNthFrame::new(7).unwrap(),
            Arc::new(TestClock::new()),
            Arc::new(TestIds::new()),
            Arc::clone(&sink),
            Arc::new(TestObserver::default()),
        );
        let capture_target = target();
        coordinator
            .start_target(
                capture_target.clone(),
                Arc::clone(&transport) as Arc<dyn CdpTransport>,
            )
            .await
            .unwrap();
        let start = transport
            .start_params()
            .into_iter()
            .next()
            .expect("capture starts after subscriptions");
        assert_eq!(start["format"], expected_format);
        assert_eq!(start["everyNthFrame"], 7);
        assert_eq!(coordinator.every_nth_frame().get(), 7);
        assert_eq!(coordinator.statuses()[0].every_nth_frame().get(), 7);
        coordinator
            .stop_target(
                &capture_target,
                CaptureStopReason::TargetClosed,
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .await;
    }
}

#[tokio::test]
async fn ack_completion_and_histogram_precede_parse_queue_and_sink() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let order = Arc::new(Mutex::new(Vec::new()));
    let transport = TestTransport::new(Arc::clone(&ack_completed), Arc::clone(&order));
    let sink = Arc::new(TestSink::new(ack_completed, Arc::clone(&order)));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(1).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        observer,
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame(-7).await;
    transport.wait_for_acks(1).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sink.first_frame_started.notified(),
    )
    .await
    .unwrap();
    let order = order.lock().unwrap().clone();
    assert_eq!(order, vec!["ack", "sink"]);
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.failure(), None);
    assert_eq!(status.statistics().acknowledged_frames(), 1);
    assert_eq!(status.ack_latency().sample_count(), 1);
    assert_eq!(*transport.ack_tokens.lock().unwrap(), vec![-7]);
    sink.release_first_frame.notify_one();
    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn default_ack_deadline_accepts_a_frame_delayed_beyond_250_milliseconds() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    transport.hold_ack();
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::new(TestObserver::default()),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.frame(1).await;
    transport.wait_for_ack_start().await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    transport.release_ack();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        transport.wait_for_acks(1),
    )
    .await
    .expect("default acknowledgement deadline exceeds 300 milliseconds");

    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.failure(), None);
    assert_eq!(status.statistics().received_frames(), 1);
    assert_eq!(status.statistics().acknowledged_frames(), 1);
    sink.release_first_frame.notify_one();
    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn constant_ack_tokens_produce_local_ordinals_without_discontinuity_gaps() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(4).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    for _ in 0..3 {
        transport.frame(1).await;
    }
    transport.wait_for_acks(3).await;
    sink.release_first_frame.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if sink.frames.lock().unwrap().len() == 3 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let (ordinals, warnings_empty) = {
        let frames = sink.frames.lock().unwrap();
        (
            frames
                .iter()
                .map(|frame| frame.metadata().capture_ordinal().get())
                .collect::<Vec<_>>(),
            frames
                .iter()
                .all(|frame| frame.metadata().warnings().is_empty()),
        )
    };
    assert_eq!(ordinals, vec![1, 2, 3]);
    assert!(warnings_empty);
    assert_eq!(transport.ack_tokens.lock().unwrap().as_slice(), &[1, 1, 1]);
    assert!(observer.gaps.lock().unwrap().is_empty());
    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn runtime_geometry_change_keeps_one_continuous_stream_and_per_frame_metadata() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(2).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport
        .frame_with_metadata(11, 1280, 720, Some(1.0))
        .await;
    transport.wait_for_acks(1).await;
    let transition = coordinator
        .begin_geometry_transition(
            capture_target.target_id,
            capture_target.attachment_generation,
        )
        .unwrap();
    assert!(coordinator.commit_geometry_transition(
        transition,
        CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(390, 844).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(3.0).unwrap(),
        },
    ));
    // pageScaleFactor remains 1.0: it is page zoom, not the authoritative device pixel ratio.
    transport.frame_with_metadata(12, 390, 844, None).await;
    transport.wait_for_acks(2).await;
    sink.release_first_frame.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    {
        let frames = sink.frames.lock().unwrap();
        assert_eq!(
            frames
                .iter()
                .map(|frame| frame.metadata().capture_ordinal().get())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            frames[0].metadata().viewport(),
            krometrail_core::PixelDimensions::new(600, 500).unwrap()
        );
        assert_eq!(frames[0].metadata().device_scale_factor().get(), 1.0);
        assert_eq!(
            frames[1].metadata().viewport(),
            krometrail_core::PixelDimensions::new(390, 844).unwrap()
        );
        assert_eq!(frames[1].metadata().device_scale_factor().get(), 3.0);
        assert!(
            frames[1]
                .metadata()
                .warnings()
                .contains(&krometrail_core::CaptureWarning::MissingSourceTime)
        );
    }
    assert!(observer.gaps.lock().unwrap().is_empty());
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.state(), CaptureStreamState::Capturing);
    assert_eq!(status.statistics().persisted_frames(), 2);

    let outcome = coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(outcome.complete);
}

#[tokio::test]
async fn acknowledgement_spanning_geometry_transition_retains_pixels_with_uncertain_metadata() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.hold_ack();
    transport.frame(31).await;
    transport.wait_for_ack_start().await;
    let transition = coordinator
        .begin_geometry_transition(
            capture_target.target_id,
            capture_target.attachment_generation,
        )
        .unwrap();
    assert!(coordinator.commit_geometry_transition(
        transition,
        CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(390, 844).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(3.0).unwrap(),
        },
    ));
    transport.release_ack();
    transport.wait_for_acks(1).await;

    sink.release_first_frame.notify_one();
    transport.frame(32).await;
    transport.wait_for_acks(2).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    {
        let frames = sink.frames.lock().unwrap();
        assert_eq!(frames[0].metadata().capture_ordinal().get(), 1);
        assert_eq!(
            frames[0].metadata().viewport(),
            krometrail_core::PixelDimensions::new(600, 500).unwrap()
        );
        assert!(
            frames[0]
                .metadata()
                .warnings()
                .contains(&krometrail_core::CaptureWarning::ViewportMetadataIncomplete)
        );
        assert_eq!(frames[1].metadata().capture_ordinal().get(), 2);
        assert_eq!(
            frames[1].metadata().viewport(),
            krometrail_core::PixelDimensions::new(390, 844).unwrap()
        );
        assert!(
            !frames[1]
                .metadata()
                .warnings()
                .contains(&krometrail_core::CaptureWarning::ViewportMetadataIncomplete)
        );
    }
    assert!(observer.gaps.lock().unwrap().is_empty());
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.statistics().acknowledged_frames(), 2);
    assert_eq!(status.statistics().accepted_frames(), 2);
    assert_eq!(status.statistics().dropped_frames(), 0);

    assert!(
        coordinator
            .stop_target(
                &capture_target,
                CaptureStopReason::TargetClosed,
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .complete
    );
}

#[tokio::test]
async fn frame_burst_during_geometry_refresh_is_retained_without_visual_gaps() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            max_active_streams: NonZeroUsize::new(1).unwrap(),
            queue_capacity: NonZeroUsize::new(16).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    let transition = coordinator
        .begin_geometry_transition(
            capture_target.target_id,
            capture_target.attachment_generation,
        )
        .unwrap();
    sink.release_first_frame.notify_one();
    for token in 1..=12 {
        transport.frame(token).await;
    }
    transport.wait_for_acks(12).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() < 12 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(coordinator.commit_geometry_transition(
        transition,
        CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(390, 844).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(3.0).unwrap(),
        },
    ));
    transport.frame(13).await;
    transport.wait_for_acks(13).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() < 13 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    {
        let frames = sink.frames.lock().unwrap();
        assert!(frames[..12].iter().all(|frame| {
            frame
                .metadata()
                .warnings()
                .contains(&krometrail_core::CaptureWarning::ViewportMetadataIncomplete)
        }));
        assert!(
            !frames[12]
                .metadata()
                .warnings()
                .contains(&krometrail_core::CaptureWarning::ViewportMetadataIncomplete)
        );
    }
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.statistics().received_frames(), 13);
    assert_eq!(status.statistics().acknowledged_frames(), 13);
    assert_eq!(status.statistics().accepted_frames(), 13);
    assert_eq!(status.statistics().persisted_frames(), 13);
    assert_eq!(status.statistics().dropped_frames(), 0);
    assert_eq!(status.statistics().gap_count(), 0);
    assert!(observer.gaps.lock().unwrap().is_empty());

    assert!(
        coordinator
            .stop_target(
                &capture_target,
                CaptureStopReason::TargetClosed,
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .complete
    );
}

#[tokio::test]
async fn unresolved_geometry_refresh_retains_frames_with_last_known_metadata() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    observer
        .defer_geometry_refresh
        .store(true, Ordering::Release);
    transport.geometry_event("Page.frameResized").await;
    observer.wait_for_geometry_refreshes(1).await;
    let transition = observer.geometry_refreshes.lock().unwrap()[0];
    transport.frame(51).await;
    transport.wait_for_acks(1).await;

    // Retry exhaustion leaves the transition unresolved. Pixels remain authoritative while the
    // last established viewport metadata is explicitly marked incomplete.
    assert_eq!(
        coordinator
            .geometry_for_test(
                capture_target.target_id,
                capture_target.attachment_generation,
            )
            .unwrap(),
        (
            CaptureGeometry {
                viewport: krometrail_core::PixelDimensions::new(600, 500).unwrap(),
                device_scale_factor: krometrail_core::DeviceScaleFactor::new(1.0).unwrap(),
            },
            true
        )
    );

    // A later browser geometry event must redispatch the still-open transition rather than
    // treating its last established geometry as a successful refresh.
    observer
        .defer_geometry_refresh
        .store(false, Ordering::Release);
    transport.geometry_event("Page.frameNavigated").await;
    observer.wait_for_geometry_refreshes(2).await;
    let retried = observer.geometry_refreshes.lock().unwrap()[1];
    assert_eq!(retried, transition);
    transport.frame(52).await;
    transport.wait_for_acks(2).await;
    assert!(coordinator.commit_geometry_transition(
        retried,
        CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(390, 844).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(3.0).unwrap(),
        },
    ));
    transport.frame_with_metadata(53, 390, 844, Some(3.0)).await;
    transport.wait_for_acks(3).await;
    sink.first_frame_started.notified().await;
    sink.release_first_frame.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() < 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let (ordinals, viewports, uncertain) = {
        let frames = sink.frames.lock().unwrap();
        (
            frames
                .iter()
                .map(|frame| frame.metadata().capture_ordinal().get())
                .collect::<Vec<_>>(),
            frames
                .iter()
                .map(|frame| frame.metadata().viewport())
                .collect::<Vec<_>>(),
            frames
                .iter()
                .map(|frame| {
                    frame
                        .metadata()
                        .warnings()
                        .contains(&krometrail_core::CaptureWarning::ViewportMetadataIncomplete)
                })
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(ordinals, vec![1, 2, 3]);
    assert_eq!(
        viewports,
        vec![
            krometrail_core::PixelDimensions::new(600, 500).unwrap(),
            krometrail_core::PixelDimensions::new(600, 500).unwrap(),
            krometrail_core::PixelDimensions::new(390, 844).unwrap(),
        ]
    );
    assert_eq!(uncertain, vec![true, true, false]);

    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.state(), CaptureStreamState::Capturing);
    assert_eq!(status.failure(), None);
    assert_eq!(status.statistics().acknowledged_frames(), 3);
    assert_eq!(status.statistics().accepted_frames(), 3);
    assert_eq!(status.statistics().dropped_frames(), 0);
    assert!(observer.gaps.lock().unwrap().is_empty());

    assert!(
        coordinator
            .stop_target(
                &capture_target,
                CaptureStopReason::TargetClosed,
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .complete
    );
}

#[tokio::test]
async fn native_resize_and_navigation_events_fence_generation_scoped_geometry_refreshes() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.geometry_event("Page.frameResized").await;
    observer.wait_for_geometry_refreshes(1).await;
    let resize = observer.geometry_refreshes.lock().unwrap()[0];
    assert_eq!(resize.target_id(), capture_target.target_id);
    assert_eq!(
        resize.attachment_generation(),
        capture_target.attachment_generation
    );
    assert!(coordinator.commit_geometry_transition(
        resize,
        CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(800, 600).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(2.0).unwrap(),
        },
    ));

    transport.geometry_event("Page.frameNavigated").await;
    observer.wait_for_geometry_refreshes(2).await;
    let navigation = observer.geometry_refreshes.lock().unwrap()[1];
    assert_ne!(navigation, resize);
    assert!(coordinator.commit_geometry_transition(
        navigation,
        CaptureGeometry {
            viewport: krometrail_core::PixelDimensions::new(1024, 768).unwrap(),
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(1.5).unwrap(),
        },
    ));

    sink.release_first_frame.notify_one();
    transport.frame(41).await;
    transport.wait_for_acks(1).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    {
        let frames = sink.frames.lock().unwrap();
        assert_eq!(
            frames[0].metadata().viewport(),
            krometrail_core::PixelDimensions::new(1024, 768).unwrap()
        );
        assert_eq!(frames[0].metadata().device_scale_factor().get(), 1.5);
    }
    assert!(observer.gaps.lock().unwrap().is_empty());

    assert!(
        coordinator
            .stop_target(
                &capture_target,
                CaptureStopReason::TargetClosed,
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .complete
    );
}

#[tokio::test]
async fn adaptive_screencast_encoding_does_not_invent_viewport_changes() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(2).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport
        .frame_with_encoding(21, 1200, 1000, 600, 500, Some(1.0))
        .await;
    transport
        .frame_with_encoding(22, 600, 500, 300, 250, Some(2.0))
        .await;
    transport.wait_for_acks(2).await;
    sink.release_first_frame.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while sink.frames.lock().unwrap().len() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let expected = krometrail_core::PixelDimensions::new(600, 500).unwrap();
    {
        let frames = sink.frames.lock().unwrap();
        assert_eq!(frames[0].metadata().viewport(), expected);
        assert_eq!(frames[1].metadata().viewport(), expected);
        assert_eq!(
            frames[0].metadata().image(),
            krometrail_core::PixelDimensions::new(1200, 1000).unwrap()
        );
        assert_eq!(frames[1].metadata().image(), expected);
        assert_eq!(frames[0].metadata().device_scale_factor().get(), 1.0);
        assert_eq!(frames[1].metadata().device_scale_factor().get(), 1.0);
    }
    assert!(observer.gaps.lock().unwrap().is_empty());
    assert_eq!(transport.start_params().len(), 1);

    let outcome = coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(outcome.complete);
}

#[tokio::test]
async fn saturated_queue_drops_after_ack_without_waiting_for_blocked_sink() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let order = Arc::new(Mutex::new(Vec::new()));
    let transport = TestTransport::new(Arc::clone(&ack_completed), order);
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(1).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame(1).await;
    sink.first_frame_started.notified().await;
    transport.frame(2).await;
    transport.frame(3).await;
    transport.wait_for_acks(3).await;
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.statistics().received_frames(), 3);
    assert_eq!(status.statistics().acknowledged_frames(), 3);
    assert_eq!(status.statistics().accepted_frames(), 2);
    assert_eq!(status.statistics().dropped_frames(), 1);
    assert_eq!(status.queue_depth(), 1);
    assert!(
        observer
            .gaps
            .lock()
            .unwrap()
            .iter()
            .any(|gap| gap.reason() == &CaptureGapReason::IngestionQueueSaturated)
    );
    sink.release_first_frame.notify_one();
    let outcome = coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(outcome.complete);
}

#[tokio::test]
async fn budget_pause_keeps_acknowledging_records_loss_and_resumes_hidden_state() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(BudgetSink::new_blocked());
    let retention = Arc::new(TestRetention::available());
    retention.pause();
    let observer = Arc::new(TestObserver::default());
    let coordinator = CaptureCoordinator::new(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(1).unwrap(),
            ..CaptureConfig::default()
        },
        krometrail_core::EveryNthFrame::default(),
        CaptureDependencies {
            clock: Arc::new(TestClock::new()),
            ids: Arc::new(TestIds::new()),
            sink: Arc::clone(&sink) as Arc<dyn RecordingSink>,
            retention: Arc::clone(&retention) as Arc<dyn RetentionStore>,
        },
        Arc::clone(&observer) as Arc<dyn CaptureObserver>,
    )
    .unwrap();
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.frame(1).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while coordinator.statuses()[0].state() != CaptureStreamState::PausedBudget {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    transport.frame(2).await;
    transport.frame(3).await;
    transport.wait_for_acks(3).await;
    transport.visibility(false).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while observer.visibility.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        coordinator.statuses()[0].state(),
        CaptureStreamState::PausedBudget
    );

    sink.resume();
    retention.resume();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let status = coordinator.statuses().remove(0);
            if status.state() == CaptureStreamState::Hidden
                && status.statistics().persisted_frames() == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let status = coordinator.statuses().remove(0);
    assert_eq!(status.statistics().acknowledged_frames(), 3);
    assert_eq!(status.statistics().accepted_frames(), 2);
    assert_eq!(status.statistics().dropped_frames(), 1);
    assert_eq!(status.statistics().gap_count(), 2);
    {
        let persisted = sink.gaps.lock().unwrap();
        assert!(persisted.iter().any(|gap| {
            gap.reason() == &CaptureGapReason::PersistenceRejected
                && gap.detail() == Some("disk budget paused capture")
        }));
        assert!(
            persisted
                .iter()
                .any(|gap| gap.reason() == &CaptureGapReason::IngestionQueueSaturated)
        );
    }

    let outcome = coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(outcome.complete);
}

#[tokio::test]
async fn stopping_while_budget_paused_cancels_the_wait() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(BudgetSink::new_blocked());
    let retention = Arc::new(TestRetention::available());
    retention.pause();
    let coordinator = CaptureCoordinator::new(
        CaptureConfig::default(),
        krometrail_core::EveryNthFrame::default(),
        CaptureDependencies {
            clock: Arc::new(TestClock::new()),
            ids: Arc::new(TestIds::new()),
            sink: sink as Arc<dyn RecordingSink>,
            retention: retention as Arc<dyn RetentionStore>,
        },
        Arc::new(TestObserver::default()),
    )
    .unwrap();
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame(1).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while coordinator.statuses()[0].state() != CaptureStreamState::PausedBudget {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        coordinator.stop_target(
            &capture_target,
            CaptureStopReason::Cancelled,
            tokio::time::Instant::now() + std::time::Duration::from_millis(150),
        ),
    )
    .await
    .expect("paused stop must not wait for budget availability");
    assert!(outcome.complete);
}

#[tokio::test]
async fn failed_ack_never_enters_accepted_or_dropped_accounting() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    transport.fail_ack.store(true, Ordering::Release);
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        sink,
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target,
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame(1).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if coordinator.statuses()[0].state() == CaptureStreamState::Failed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(
        status.failure().map(krometrail_core::CaptureFailure::stage),
        Some(krometrail_core::CaptureFailureStage::Acknowledgement)
    );
    assert_eq!(status.statistics().received_frames(), 1);
    assert_eq!(status.statistics().acknowledged_frames(), 0);
    assert_eq!(status.statistics().accepted_frames(), 0);
    assert_eq!(status.statistics().dropped_frames(), 0);
    let gaps = observer.gaps.lock().unwrap();
    let acknowledgement_gap = gaps
        .iter()
        .find(|gap| gap.reason() == &CaptureGapReason::AcknowledgementFailed)
        .expect("acknowledgement failure must declare a gap");
    assert_eq!(
        acknowledgement_gap
            .estimated_missing_frames()
            .unwrap()
            .get(),
        1
    );
}

#[tokio::test]
async fn acknowledgement_beyond_an_explicit_short_deadline_fails_once_with_one_gap() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    transport.hold_ack();
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            ack_timeout: std::time::Duration::from_millis(20),
            ..CaptureConfig::default()
        },
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        sink,
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target,
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.frame(7).await;
    transport.wait_for_ack_start().await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while coordinator.statuses()[0].state() != CaptureStreamState::Failed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("short acknowledgement deadline is terminal");

    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(
        status.failure().map(krometrail_core::CaptureFailure::stage),
        Some(krometrail_core::CaptureFailureStage::Acknowledgement)
    );
    assert_eq!(status.statistics().received_frames(), 1);
    assert_eq!(status.statistics().acknowledged_frames(), 0);
    assert_eq!(*transport.ack_tokens.lock().unwrap(), vec![7]);
    let gaps = observer.gaps.lock().unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].reason(), &CaptureGapReason::AcknowledgementFailed);
    drop(gaps);
    assert_eq!(*observer.capture_failures.lock().unwrap(), vec![1]);
}

#[tokio::test]
async fn visibility_intervals_coalesce_and_actual_visibility_recovers() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        sink,
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.visibility(false).await;
    transport.visibility(false).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if coordinator.statuses()[0].state() == CaptureStreamState::Hidden {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(observer.gaps.lock().unwrap().len(), 1);
    transport.visibility(true).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if coordinator.statuses()[0].state() == CaptureStreamState::Capturing {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn visibility_observations_are_stamped_at_reader_dequeue() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport = TestTransport::new(ack_completed, Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        Arc::new(AtomicBool::new(true)),
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        sink,
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.visibility(false).await;
    transport.visibility(true).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while observer.visibility_times.lock().unwrap().len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let visibility_times = observer.visibility_times.lock().unwrap().clone();
    assert_eq!(visibility_times.len(), 2);
    assert!(visibility_times[0] < visibility_times[1]);
    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn visibility_stamp_failure_drops_without_notifying_observer() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport = TestTransport::new(ack_completed, Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        Arc::new(AtomicBool::new(true)),
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        sink,
        Arc::clone(&observer),
    );
    let mut capture_target = target();
    capture_target.session_origin = SessionOrigin::new(ObservedTime::from_nanos(2));
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    transport.visibility(false).await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(observer.visibility.lock().unwrap().is_empty());
    assert!(observer.visibility_times.lock().unwrap().is_empty());
    assert_eq!(
        coordinator.statuses()[0].state(),
        CaptureStreamState::Capturing
    );

    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[test]
fn equal_or_backwards_clock_samples_are_clamped_without_reordering() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let runtime = pipeline::StreamRuntime::new(
        target(),
        CaptureConfig::default(),
        krometrail_core::EveryNthFrame::default(),
        CaptureDependencies {
            clock: Arc::new(TestClock::new()),
            ids: Arc::new(TestIds::new()),
            sink,
            retention: Arc::new(TestRetention::available()),
        },
        observer,
        transport,
        Arc::new(pipeline::OrdinalRegistry::default()),
    );
    runtime.record_received();
    let (first_observed, first_session) = runtime.record_ack(0, ObservedTime::from_nanos(10));
    runtime.record_received();
    let (second_observed, second_session) = runtime.record_ack(0, ObservedTime::from_nanos(9));
    assert_eq!(first_observed, ObservedTime::from_nanos(10));
    assert_eq!(second_observed, first_observed);
    assert_eq!(second_session, first_session);
    assert_eq!(runtime.status().frame_cadence().max_nanos(), Some(0));
}

#[test]
fn ordinal_allocation_is_strict_and_fenced_across_attachment_generations() {
    let registry = pipeline::OrdinalRegistry::default();
    let first_target = target();
    assert!(registry.begin_generation(&first_target));
    assert_eq!(
        registry.allocate(&first_target),
        pipeline::OrdinalAllocation::Allocated(krometrail_core::CaptureOrdinal::new(1).unwrap())
    );
    assert_eq!(
        registry.allocate(&first_target),
        pipeline::OrdinalAllocation::Allocated(krometrail_core::CaptureOrdinal::new(2).unwrap())
    );

    let mut restored_target = first_target.clone();
    restored_target.attachment_generation = 2;
    assert!(registry.begin_generation(&restored_target));
    assert_eq!(
        registry.allocate(&first_target),
        pipeline::OrdinalAllocation::StaleGeneration
    );
    assert_eq!(
        registry.allocate(&restored_target),
        pipeline::OrdinalAllocation::Allocated(krometrail_core::CaptureOrdinal::new(3).unwrap())
    );
}

#[tokio::test]
async fn blocked_in_flight_work_is_reported_as_bounded_stop_abandonment() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame(1).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sink.first_frame_started.notified(),
    )
    .await
    .unwrap();
    let outcome = coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::Cancelled,
            tokio::time::Instant::now() + std::time::Duration::from_millis(20),
        )
        .await;
    assert!(!outcome.complete);
    assert_eq!(outcome.abandoned_accepted_frames, 1);
    assert!(
        observer
            .gaps
            .lock()
            .unwrap()
            .iter()
            .any(|gap| gap.reason() == &CaptureGapReason::CaptureStopped)
    );
}

#[test]
fn status_and_gap_serialization_are_privacy_safe() {
    let source = include_str!("pipeline.rs");
    assert!(!source.contains("tracing::info!"));
    assert!(!source.contains("tracing::debug!"));
    assert!(!source.contains("page title"));
    assert!(!source.contains("browser target key"));
    assert!(!source.contains("tracing::warn!(?"));
    assert!(!source.contains("tracing::warn!(error"));
    assert!(source.contains("failure_stage"));
    assert!(source.contains("error_code"));
    assert!(source.contains("capture.ack.failed"));
    assert!(source.contains("deadline_nanos"));
    assert!(source.contains("elapsed_nanos"));
    assert!(source.contains("received_frames"));
    assert!(source.contains("acknowledged_frames"));
    let status = serde_json::to_string(&CaptureStreamState::Capturing).unwrap();
    assert_eq!(status, "\"capturing\"");
}

#[tokio::test]
async fn shutdown_flushes_the_session_sink_once_after_target_stop() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        observer,
    );
    coordinator
        .start_target(target(), Arc::clone(&transport) as Arc<dyn CdpTransport>)
        .await
        .unwrap();
    let result = coordinator
        .shutdown(
            SessionId::from_uuid(uuid::Uuid::from_u128(1)),
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(result.complete);
    assert!(result.flush_attempted);
    assert!(result.flush_succeeded);
    assert_eq!(sink.flushes.load(Ordering::Relaxed), 1);
}

#[test]
fn gap_ledger_coalesces_without_growing_with_loss_count() {
    let mut ledger = pipeline::GapLedger::new(2);
    let session = SessionId::from_uuid(uuid::Uuid::from_u128(1));
    let target = TargetId::from_uuid(uuid::Uuid::from_u128(2));
    let make_gap = |id: u128, at: u64| {
        krometrail_core::CaptureGap::new(
            krometrail_core::GapId::from_uuid(uuid::Uuid::from_u128(id)),
            session,
            target,
            krometrail_core::SessionRange::new(
                krometrail_core::SessionTime::from_nanos(at),
                krometrail_core::SessionTime::from_nanos(at),
            )
            .unwrap(),
            krometrail_core::ObservedTime::from_nanos(at),
            CaptureGapReason::IngestionQueueSaturated,
            std::num::NonZeroU64::new(1),
            None,
        )
        .unwrap()
    };
    ledger.push(make_gap(1, 1));
    ledger.push(make_gap(2, 2));
    ledger.push(make_gap(3, 3));
    assert!(ledger.pending.len() <= 2);
    let total: u64 = ledger
        .pending
        .iter()
        .map(|gap| gap.estimated_missing_frames().unwrap().get())
        .sum();
    assert_eq!(total, 3);
}

#[tokio::test]
async fn ack_latency_uses_receipt_sample_and_excludes_wait_and_post_ack_work() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let order = Arc::new(Mutex::new(Vec::new()));
    let transport = TestTransport::new(Arc::clone(&ack_completed), Arc::clone(&order));
    let sink = Arc::new(TestSink::new(ack_completed, Arc::clone(&order)));
    let clock = Arc::new(TestClock::with_stride(10));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig {
            queue_capacity: NonZeroUsize::new(4).unwrap(),
            ..CaptureConfig::default()
        },
        Arc::clone(&clock),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    let base_calls = clock.calls();
    for token in 1..=3 {
        transport.frame(token).await;
    }
    transport.wait_for_acks(3).await;
    // Two deterministic clock samples per acknowledged frame: the receipt sample and the
    // ack-completion sample. There is no intermediate sample after token extraction.
    assert_eq!(clock.calls(), base_calls + 6);
    let status = coordinator.statuses().pop().unwrap();
    assert_eq!(status.statistics().acknowledged_frames(), 3);
    assert_eq!(status.ack_latency().sample_count(), 3);
    assert_eq!(status.ack_latency().max_nanos(), Some(10));
    sink.release_first_frame.notify_one();
    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn terminal_stop_publishes_final_status_before_runtime_removal() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        Arc::new(TestClock::new()),
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let capture_target = target();
    coordinator
        .start_target(
            capture_target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    sink.release_first_frame.notify_one();
    transport.frame(1).await;
    transport.frame(2).await;
    transport.wait_for_acks(2).await;

    coordinator
        .stop_target(
            &capture_target,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;

    let statuses = observer.statuses.lock().unwrap();
    let final_status = statuses
        .iter()
        .rev()
        .find(|status| status.target_id() == capture_target.target_id)
        .expect("terminal stop must publish a final status event");
    assert_eq!(final_status.state(), CaptureStreamState::Stopped);
    assert_eq!(final_status.statistics().received_frames(), 2);
    assert_eq!(final_status.statistics().acknowledged_frames(), 2);
    drop(statuses);
    assert!(
        coordinator.statuses().is_empty(),
        "stopped runtime must be removed from the registry"
    );
}

#[tokio::test]
async fn repeated_target_churn_keeps_registry_and_statuses_bounded() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let clock = Arc::new(TestClock::new());
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        clock,
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let target_id = TargetId::from_uuid(uuid::Uuid::from_u128(42));
    sink.release_first_frame.notify_one();
    for generation in 1..=10 {
        let session = format!("churn-session-{generation}");
        let transport_session = TransportSessionId::new(&session).unwrap();
        transport.ensure_session(&transport_session);
        let target = CaptureTarget {
            session_id: SessionId::from_uuid(uuid::Uuid::from_u128(1)),
            session_origin: SessionOrigin::new(ObservedTime::from_nanos(0)),
            target_id,
            connection_generation: 1,
            attachment_generation: generation,
            transport_session,
            geometry: CaptureGeometry {
                viewport: krometrail_core::PixelDimensions::new(600, 500).unwrap(),
                device_scale_factor: krometrail_core::DeviceScaleFactor::new(1.0).unwrap(),
            },
        };
        coordinator
            .start_target(
                target.clone(),
                Arc::clone(&transport) as Arc<dyn CdpTransport>,
            )
            .await
            .unwrap();
        transport.frame_for(&target.transport_session, 1).await;
        transport.wait_for_acks(generation as usize).await;
        let statuses = coordinator.statuses();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].attachment_generation(), generation);
        coordinator
            .stop_target(
                &target,
                CaptureStopReason::TargetClosed,
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .await;
        assert!(coordinator.statuses().is_empty());
    }
    let ordinals: Vec<_> = sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .map(|frame| frame.metadata().capture_ordinal().get())
        .collect();
    assert_eq!(ordinals, vec![1; 10]);
}

#[tokio::test]
async fn target_detach_preserves_ordinal_continuity() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let clock = Arc::new(TestClock::new());
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        clock,
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let target = target();
    coordinator
        .start_target(
            target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    sink.release_first_frame.notify_one();
    transport.frame(1).await;
    transport.frame(2).await;
    transport.wait_for_acks(2).await;
    coordinator
        .stop_target(
            &target,
            CaptureStopReason::TargetDetached,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;

    let resumed = target_with(2, "transport-session-resume", 2);
    transport.ensure_session(&resumed.transport_session);
    coordinator
        .start_target(
            resumed.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame_for(&resumed.transport_session, 3).await;
    transport.wait_for_acks(3).await;
    let ordinals: Vec<_> = sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .map(|frame| frame.metadata().capture_ordinal().get())
        .collect();
    assert_eq!(ordinals, vec![1, 2, 3]);
    coordinator
        .stop_target(
            &resumed,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn statuses_expose_highest_generation_per_target_sorted() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let clock = Arc::new(TestClock::new());
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        clock,
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let target_a = target_with(10, "session-a", 1);
    let target_b = target_with(20, "session-b", 1);
    transport.ensure_session(&target_a.transport_session);
    transport.ensure_session(&target_b.transport_session);
    coordinator
        .start_target(
            target_a.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    coordinator
        .start_target(
            target_b.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    let target_a2 = target_with(10, "session-a-2", 2);
    transport.ensure_session(&target_a2.transport_session);
    coordinator
        .start_target(
            target_a2.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();

    let statuses = coordinator.statuses();
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].target_id(), target_a.target_id);
    assert_eq!(statuses[0].attachment_generation(), 2);
    assert_eq!(statuses[1].target_id(), target_b.target_id);
    assert_eq!(statuses[1].attachment_generation(), 1);

    coordinator
        .stop_target(
            &target_a2,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    coordinator
        .stop_target(
            &target_a,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    coordinator
        .stop_target(
            &target_b,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(coordinator.statuses().is_empty());
}

#[tokio::test]
async fn stop_removes_exact_runtime_without_erasing_newer_generation() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let clock = Arc::new(TestClock::new());
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        clock,
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let gen1 = target_with(5, "session-gen1", 1);
    let gen2 = target_with(5, "session-gen2", 2);
    transport.ensure_session(&gen1.transport_session);
    transport.ensure_session(&gen2.transport_session);
    coordinator
        .start_target(
            gen1.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    coordinator
        .start_target(
            gen2.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    sink.release_first_frame.notify_one();

    coordinator
        .stop_target(
            &gen1,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    let statuses = coordinator.statuses();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].attachment_generation(), 2);

    transport.frame_for(&gen2.transport_session, 1).await;
    transport.wait_for_acks(1).await;
    let ordinals: Vec<_> = sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .map(|frame| frame.metadata().capture_ordinal().get())
        .collect();
    assert_eq!(ordinals, vec![1]);
    coordinator
        .stop_target(
            &gen2,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;

    let gen3 = target_with(5, "session-gen3", 3);
    transport.ensure_session(&gen3.transport_session);
    coordinator
        .start_target(
            gen3.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame_for(&gen3.transport_session, 1).await;
    transport.wait_for_acks(2).await;
    let ordinals: Vec<_> = sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .map(|frame| frame.metadata().capture_ordinal().get())
        .collect();
    assert_eq!(ordinals, vec![1, 1]);
    coordinator
        .stop_target(
            &gen3,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}

#[tokio::test]
async fn session_shutdown_clears_ordinal_registry() {
    let ack_completed = Arc::new(AtomicBool::new(false));
    let transport =
        TestTransport::new(Arc::clone(&ack_completed), Arc::new(Mutex::new(Vec::new())));
    let sink = Arc::new(TestSink::new(
        ack_completed,
        Arc::new(Mutex::new(Vec::new())),
    ));
    let clock = Arc::new(TestClock::new());
    let observer = Arc::new(TestObserver::default());
    let coordinator = coordinator(
        CaptureConfig::default(),
        clock,
        Arc::new(TestIds::new()),
        Arc::clone(&sink),
        Arc::clone(&observer),
    );
    let target = target();
    coordinator
        .start_target(
            target.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    sink.release_first_frame.notify_one();
    transport.frame(1).await;
    transport.wait_for_acks(1).await;
    let outcome = coordinator
        .shutdown(
            SessionId::from_uuid(uuid::Uuid::from_u128(1)),
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
    assert!(outcome.complete);
    assert!(coordinator.statuses().is_empty());

    let restarted = target_with(2, "transport-session-restart", 2);
    transport.ensure_session(&restarted.transport_session);
    coordinator
        .start_target(
            restarted.clone(),
            Arc::clone(&transport) as Arc<dyn CdpTransport>,
        )
        .await
        .unwrap();
    transport.frame_for(&restarted.transport_session, 1).await;
    transport.wait_for_acks(2).await;
    let ordinals: Vec<_> = sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .map(|frame| frame.metadata().capture_ordinal().get())
        .collect();
    assert_eq!(ordinals, vec![1, 1]);
    coordinator
        .stop_target(
            &restarted,
            CaptureStopReason::TargetClosed,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
}
