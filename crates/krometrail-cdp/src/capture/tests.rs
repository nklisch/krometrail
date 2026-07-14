use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ByteOffset, CaptureGapReason, CaptureStreamState, DiskBudgetBytes, EncodedFrame, ErrorCode,
    FrameAddress, IdValue, ImageFormat, KrometrailError, MonotonicClock, NonEmptyText,
    ObservedTime, PinChange, PortFuture, RecordingSink, RetentionRange, RetentionStatus,
    RetentionStore, SegmentId, SessionDeletion, SessionId, SessionOrigin, TargetId,
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
}

impl CaptureObserver for TestObserver {
    fn status_changed(&self, status: krometrail_core::TargetCaptureStatus) {
        self.statuses.lock().unwrap().push(status);
    }

    fn gap_declared(&self, gap: krometrail_core::CaptureGap) {
        self.gaps.lock().unwrap().push(gap);
    }

    fn visibility_changed(
        &self,
        _target_id: TargetId,
        visibility: krometrail_core::TargetVisibility,
    ) {
        self.visibility.lock().unwrap().push(visibility);
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
    calls: Mutex<Vec<String>>,
    ack_watch: watch::Sender<usize>,
    ack_count: AtomicU64,
    ack_tokens: Mutex<Vec<i64>>,
    ack_completed: Arc<AtomicBool>,
    order: Arc<Mutex<Vec<&'static str>>>,
    fail_ack: AtomicBool,
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
        let (ack_watch, _) = watch::channel(0);
        Arc::new(Self {
            default_session,
            frame_senders: Mutex::new(frame_senders),
            visibility_senders: Mutex::new(visibility_senders),
            frame_receivers: Mutex::new(frame_receivers),
            visibility_receivers: Mutex::new(visibility_receivers),
            calls: Mutex::new(Vec::new()),
            ack_watch,
            ack_count: AtomicU64::new(0),
            ack_tokens: Mutex::new(Vec::new()),
            ack_completed,
            order,
            fail_ack: AtomicBool::new(false),
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
    }

    async fn frame(&self, ack_token: i64) {
        self.frame_for(&self.default_session, ack_token).await;
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

    async fn wait_for_acks(&self, count: usize) {
        let mut receiver = self.ack_watch.subscribe();
        while *receiver.borrow() < count {
            receiver.changed().await.unwrap();
        }
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
        if method == "Page.screencastFrameAck" {
            if let Some(token) = params.get("sessionId").and_then(serde_json::Value::as_i64) {
                self.ack_tokens.lock().unwrap().push(token);
            }
            if self.fail_ack.load(Ordering::Acquire) {
                return Box::pin(std::future::ready(Err(TransportError::CommandFailed)));
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
    serde_json::json!({
        "data": STANDARD.encode(jpeg_bytes()),
        "metadata": {
            "deviceWidth": 640,
            "deviceHeight": 480,
            "pageScaleFactor": 1.0,
            "timestamp": 1.25
        },
        "sessionId": ack_token
    })
}

fn jpeg_bytes() -> Vec<u8> {
    vec![
        0xff, 0xd8, 0xff, 0xc0, 0, 8, 8, 1, 0, 2, 0, 2, 1, 0xff, 0xd9,
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
    }
}

fn coordinator(
    config: CaptureConfig,
    clock: Arc<TestClock>,
    ids: Arc<TestIds>,
    sink: Arc<TestSink>,
    observer: Arc<TestObserver>,
) -> CaptureCoordinator {
    CaptureCoordinator::new(
        config,
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
