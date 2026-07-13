use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    CaptureGap, CaptureGapReason, CaptureOrdinal, CaptureStatistics, CaptureStreamState,
    CaptureTimingSummary, CaptureWarning, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId,
    GapId, ImageFormat, PixelDimensions, SessionRange, SessionTime, SourceTime,
    TargetCaptureStatus,
};
use serde_json::{Value, json};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{self, Instant},
};

use super::{
    CaptureCoordinator, CaptureDependencies, CaptureError, CaptureObserver, CaptureStopOutcome,
    CaptureStopReason, CaptureTarget, StreamKey,
};
use crate::transport::{CdpTransport, CommandScope, NamedEvent, TransportEvents};

const FRAME_EVENT: &str = "Page.screencastFrame";
const VISIBILITY_EVENT: &str = "Page.screencastVisibilityChanged";
const START_METHOD: &str = "Page.startScreencast";
const STOP_METHOD: &str = "Page.stopScreencast";
const ACK_METHOD: &str = "Page.screencastFrameAck";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct OrdinalKey {
    session_id: krometrail_core::SessionId,
    target_id: krometrail_core::TargetId,
}

#[derive(Debug)]
struct OrdinalState {
    attachment_generation: u64,
    last: u64,
}

#[derive(Debug, Default)]
pub(super) struct OrdinalRegistry {
    states: Mutex<HashMap<OrdinalKey, OrdinalState>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OrdinalAllocation {
    Allocated(CaptureOrdinal),
    StaleGeneration,
    Exhausted,
}

impl OrdinalRegistry {
    pub(super) fn begin_generation(&self, target: &CaptureTarget) -> bool {
        let key = OrdinalKey {
            session_id: target.session_id,
            target_id: target.target_id,
        };
        let mut states = self.states.lock().expect("ordinal registry lock poisoned");
        match states.get_mut(&key) {
            Some(state) if target.attachment_generation <= state.attachment_generation => false,
            Some(state) => {
                state.attachment_generation = target.attachment_generation;
                true
            }
            None => {
                states.insert(
                    key,
                    OrdinalState {
                        attachment_generation: target.attachment_generation,
                        last: 0,
                    },
                );
                true
            }
        }
    }

    pub(super) fn allocate(&self, target: &CaptureTarget) -> OrdinalAllocation {
        let key = OrdinalKey {
            session_id: target.session_id,
            target_id: target.target_id,
        };
        let mut states = self.states.lock().expect("ordinal registry lock poisoned");
        let Some(state) = states.get_mut(&key) else {
            return OrdinalAllocation::StaleGeneration;
        };
        // Check the attachment fence while holding the same lock as the increment. An old reader
        // may finish acknowledging a frame during reconnect, but it cannot allocate after a newer
        // generation has been installed or race the new generation's ordinal.
        if state.attachment_generation != target.attachment_generation {
            return OrdinalAllocation::StaleGeneration;
        }
        let Some(next) = state.last.checked_add(1) else {
            return OrdinalAllocation::Exhausted;
        };
        let Ok(ordinal) = CaptureOrdinal::new(next) else {
            return OrdinalAllocation::Exhausted;
        };
        state.last = next;
        OrdinalAllocation::Allocated(ordinal)
    }

    /// Remove per-target ordinal state only for the exact terminal generation. A newer generation
    /// (for example, a concurrent reconnect replacement) must retain its continuity.
    pub(super) fn end_generation(&self, target: &CaptureTarget) {
        let key = OrdinalKey {
            session_id: target.session_id,
            target_id: target.target_id,
        };
        let mut states = self.states.lock().expect("ordinal registry lock poisoned");
        if let Some(state) = states.get(&key) {
            if state.attachment_generation <= target.attachment_generation {
                states.remove(&key);
            }
        }
    }

    pub(super) fn clear(&self) {
        self.states
            .lock()
            .expect("ordinal registry lock poisoned")
            .clear();
    }
}

pub(super) struct StreamRuntime {
    target: CaptureTarget,
    ordinals: Arc<OrdinalRegistry>,
    config: super::CaptureConfig,
    dependencies: CaptureDependencies,
    observer: Arc<dyn CaptureObserver>,
    transport: Arc<dyn CdpTransport>,
    accepting: AtomicBool,
    state: Mutex<RuntimeState>,
    control: Mutex<ControlHandles>,
}

struct ControlHandles {
    sender: Option<mpsc::Sender<RawFrame>>,
    frame_reader: Option<JoinHandle<()>>,
    visibility_reader: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
}

struct RuntimeState {
    state: CaptureStreamState,
    statistics: CaptureStatistics,
    queue_capacity: usize,
    queue_depth: usize,
    in_flight: usize,
    last_frame_session_time: Option<SessionTime>,
    previous_observed: Option<krometrail_core::ObservedTime>,
    ack_latency: Histogram,
    frame_cadence: Histogram,
    gaps: GapLedger,
}

#[derive(Clone, Debug)]
struct RawFrame {
    capture_ordinal: CaptureOrdinal,
    data: String,
    source_time: Option<SourceTime>,
    observed_time: krometrail_core::ObservedTime,
    session_time: SessionTime,
    format: ImageFormat,
    viewport: PixelDimensions,
    device_scale_factor: DeviceScaleFactor,
    warnings: Vec<CaptureWarning>,
}

#[derive(Clone, Debug)]
pub(super) struct GapLedger {
    capacity: usize,
    pub(super) pending: VecDeque<CaptureGap>,
    saturation_open: bool,
}

#[derive(Clone, Debug)]
pub(super) struct Histogram {
    pub(super) buckets: [u64; 64],
    sample_count: u64,
    max_nanos: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Transition {
    StartedVisible,
    StartedHidden,
    Hide,
    Show,
    ActualFrame,
    Suspend,
    Resume,
    Stop,
    Drained,
    Deadline,
    Failure,
}

impl StreamRuntime {
    pub(super) fn new(
        target: CaptureTarget,
        config: super::CaptureConfig,
        dependencies: CaptureDependencies,
        observer: Arc<dyn CaptureObserver>,
        transport: Arc<dyn CdpTransport>,
        ordinals: Arc<OrdinalRegistry>,
    ) -> Self {
        Self {
            target,
            ordinals,
            config: config.clone(),
            dependencies,
            observer,
            transport,
            accepting: AtomicBool::new(true),
            state: Mutex::new(RuntimeState {
                state: CaptureStreamState::Starting,
                statistics: CaptureStatistics::default(),
                queue_capacity: config.queue_capacity.get(),
                queue_depth: 0,
                in_flight: 0,
                last_frame_session_time: None,
                previous_observed: None,
                ack_latency: Histogram::default(),
                frame_cadence: Histogram::default(),
                gaps: GapLedger::new(config.gap_ledger_capacity.get()),
            }),
            control: Mutex::new(ControlHandles {
                sender: None,
                frame_reader: None,
                visibility_reader: None,
                worker: None,
            }),
        }
    }

    fn key(&self) -> StreamKey {
        StreamKey {
            target_id: self.target.target_id,
            attachment_generation: self.target.attachment_generation,
        }
    }

    fn set_sender(&self, sender: mpsc::Sender<RawFrame>) {
        self.control
            .lock()
            .expect("capture control lock poisoned")
            .sender = Some(sender);
    }

    fn set_tasks(
        &self,
        frame_reader: JoinHandle<()>,
        visibility_reader: JoinHandle<()>,
        worker: JoinHandle<()>,
    ) {
        let mut control = self.control.lock().expect("capture control lock poisoned");
        control.frame_reader = Some(frame_reader);
        control.visibility_reader = Some(visibility_reader);
        control.worker = Some(worker);
    }

    fn close_acceptance(&self) {
        self.accepting.store(false, Ordering::Release);
        self.control
            .lock()
            .expect("capture control lock poisoned")
            .sender = None;
    }

    fn abort_readers(&self) {
        let control = &mut *self.control.lock().expect("capture control lock poisoned");
        if let Some(handle) = control.frame_reader.take() {
            handle.abort();
        }
        if let Some(handle) = control.visibility_reader.take() {
            handle.abort();
        }
    }

    fn take_worker(&self) -> Option<JoinHandle<()>> {
        self.control
            .lock()
            .expect("capture control lock poisoned")
            .worker
            .take()
    }

    fn transition(&self, transition: Transition) -> bool {
        let status = {
            let mut state = self.state.lock().expect("capture state lock poisoned");
            let next = next_state(state.state, transition);
            let Some(next) = next else { return false };
            if next == state.state {
                return false;
            }
            state.state = next;
            status_from_state(&self.target, &state).expect("runtime state maintains invariants")
        };
        self.observer.status_changed(status);
        true
    }

    pub(super) fn state(&self) -> CaptureStreamState {
        self.state
            .lock()
            .expect("capture state lock poisoned")
            .state
    }

    pub(super) fn record_received(&self) {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        let statistics = CaptureStatistics::new(
            state.statistics.received_frames().saturating_add(1),
            state.statistics.acknowledged_frames(),
            state.statistics.accepted_frames(),
            state.statistics.dropped_frames(),
            state.statistics.persisted_frames(),
            state.statistics.gap_count(),
        )
        .expect("capture counters cannot overflow in a bounded process");
        state.statistics = statistics;
    }

    fn session_time_for(&self, observed: krometrail_core::ObservedTime) -> SessionTime {
        let normalized = self
            .target
            .session_origin
            .normalize(observed)
            .unwrap_or(SessionTime::ZERO);
        self.state
            .lock()
            .expect("capture state lock poisoned")
            .last_frame_session_time
            .map_or(normalized, |previous| previous.max(normalized))
    }

    pub(super) fn record_ack(
        &self,
        latency_nanos: u64,
        observed: krometrail_core::ObservedTime,
    ) -> (krometrail_core::ObservedTime, SessionTime) {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        let observed = state
            .previous_observed
            .map_or(observed, |previous| previous.max(observed));
        let mut session = self
            .target
            .session_origin
            .normalize(observed)
            .unwrap_or(SessionTime::ZERO);
        if let Some(previous) = state.last_frame_session_time {
            session = session.max(previous);
            if let Some(previous_observed) = state.previous_observed {
                let cadence = observed
                    .as_nanos()
                    .saturating_sub(previous_observed.as_nanos());
                state.frame_cadence.record(cadence);
            }
        }
        state.previous_observed = Some(observed);
        state.last_frame_session_time = Some(session);
        state.ack_latency.record(latency_nanos);
        state.statistics = CaptureStatistics::new(
            state.statistics.received_frames(),
            state.statistics.acknowledged_frames().saturating_add(1),
            state.statistics.accepted_frames(),
            state.statistics.dropped_frames(),
            state.statistics.persisted_frames(),
            state.statistics.gap_count(),
        )
        .expect("capture counters cannot overflow in a bounded process");
        (observed, session)
    }

    fn handoff(&self, raw: RawFrame) {
        let result = {
            let control = self.control.lock().expect("capture control lock poisoned");
            match control.sender.as_ref() {
                Some(sender) if self.accepting.load(Ordering::Acquire) => sender.try_send(raw),
                Some(_) | None => {
                    return self.dropped(CaptureGapReason::CaptureStopped, raw.session_time);
                }
            }
        };
        match result {
            Ok(()) => {
                let mut state = self.state.lock().expect("capture state lock poisoned");
                state.queue_depth = state.queue_depth.saturating_add(1);
                state.gaps.close_saturation();
                state.statistics = CaptureStatistics::new(
                    state.statistics.received_frames(),
                    state.statistics.acknowledged_frames(),
                    state.statistics.accepted_frames().saturating_add(1),
                    state.statistics.dropped_frames(),
                    state.statistics.persisted_frames(),
                    state.statistics.gap_count(),
                )
                .expect("capture counters cannot overflow in a bounded process");
            }
            Err(mpsc::error::TrySendError::Full(raw)) => {
                self.dropped(CaptureGapReason::IngestionQueueSaturated, raw.session_time);
            }
            Err(mpsc::error::TrySendError::Closed(raw)) => {
                self.dropped(CaptureGapReason::CaptureStopped, raw.session_time);
            }
        }
    }

    fn dropped(&self, reason: CaptureGapReason, at: SessionTime) {
        let _ = self.declare_gap(reason, at, Some(1), None);
        let mut state = self.state.lock().expect("capture state lock poisoned");
        state.statistics = CaptureStatistics::new(
            state.statistics.received_frames(),
            state.statistics.acknowledged_frames(),
            state.statistics.accepted_frames(),
            state.statistics.dropped_frames().saturating_add(1),
            state.statistics.persisted_frames(),
            state.statistics.gap_count(),
        )
        .expect("capture counters cannot overflow in a bounded process");
    }

    fn begin_processing(&self) {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        state.queue_depth = state.queue_depth.saturating_sub(1);
        state.in_flight = state.in_flight.saturating_add(1);
    }

    fn complete_processing(&self) {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        state.in_flight = state.in_flight.saturating_sub(1);
    }

    fn persisted(&self) {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        state.statistics = CaptureStatistics::new(
            state.statistics.received_frames(),
            state.statistics.acknowledged_frames(),
            state.statistics.accepted_frames(),
            state.statistics.dropped_frames(),
            state.statistics.persisted_frames().saturating_add(1),
            state.statistics.gap_count(),
        )
        .expect("persisted count cannot exceed accepted count");
    }

    fn declare_gap(
        &self,
        reason: CaptureGapReason,
        at: SessionTime,
        estimated: Option<u64>,
        detail: Option<&'static str>,
    ) -> Option<CaptureGap> {
        let gap = CaptureGap::new(
            GapId::from_uuid(*self.dependencies.ids.next().as_uuid()),
            self.target.session_id,
            self.target.target_id,
            SessionRange::new(at, at).expect("equal range is valid"),
            reason,
            estimated.and_then(std::num::NonZeroU64::new),
            detail.map(str::to_owned),
        )
        .ok()?;
        let notified = {
            let mut state = self.state.lock().expect("capture state lock poisoned");
            state.statistics = CaptureStatistics::new(
                state.statistics.received_frames(),
                state.statistics.acknowledged_frames(),
                state.statistics.accepted_frames(),
                state.statistics.dropped_frames(),
                state.statistics.persisted_frames(),
                state.statistics.gap_count().saturating_add(1),
            )
            .expect("gap count cannot overflow in a bounded process");
            state.gaps.push(gap)
        };
        self.observer.gap_declared(notified.clone());
        Some(notified)
    }

    fn declare_gap_range(
        &self,
        reason: CaptureGapReason,
        start: SessionTime,
        end: SessionTime,
        estimated: Option<u64>,
        detail: Option<&'static str>,
    ) -> Option<CaptureGap> {
        let gap = CaptureGap::new(
            GapId::from_uuid(*self.dependencies.ids.next().as_uuid()),
            self.target.session_id,
            self.target.target_id,
            SessionRange::new(start.min(end), start.max(end)).expect("ordered range is valid"),
            reason,
            estimated.and_then(std::num::NonZeroU64::new),
            detail.map(str::to_owned),
        )
        .ok()?;
        let notified = {
            let mut state = self.state.lock().expect("capture state lock poisoned");
            state.statistics = CaptureStatistics::new(
                state.statistics.received_frames(),
                state.statistics.acknowledged_frames(),
                state.statistics.accepted_frames(),
                state.statistics.dropped_frames(),
                state.statistics.persisted_frames(),
                state.statistics.gap_count().saturating_add(1),
            )
            .expect("gap count cannot overflow in a bounded process");
            state.gaps.push(gap)
        };
        self.observer.gap_declared(notified.clone());
        Some(notified)
    }

    fn take_gaps(&self) -> Vec<CaptureGap> {
        self.state
            .lock()
            .expect("capture state lock poisoned")
            .gaps
            .take()
    }

    fn abandon_queue(&self) -> u64 {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        let abandoned = state.queue_depth.saturating_add(state.in_flight) as u64;
        state.queue_depth = 0;
        state.in_flight = 0;
        abandoned
    }

    pub(super) fn status(&self) -> TargetCaptureStatus {
        let state = self.state.lock().expect("capture state lock poisoned");
        status_from_state(&self.target, &state).expect("runtime state maintains invariants")
    }

    fn fail(&self) {
        self.accepting.store(false, Ordering::Release);
        let _ = self.transition(Transition::Failure);
        self.control
            .lock()
            .expect("capture control lock poisoned")
            .sender = None;
    }
}

pub(super) async fn start_target(
    coordinator: &CaptureCoordinator,
    target: CaptureTarget,
    transport: Arc<dyn CdpTransport>,
) -> Result<(), CaptureError> {
    if target.attachment_generation == 0 {
        return Err(CaptureError::InvalidConfig(
            "capture attachment generation must be non-zero",
        ));
    }
    let key = StreamKey {
        target_id: target.target_id,
        attachment_generation: target.attachment_generation,
    };
    {
        let streams = coordinator
            .streams
            .lock()
            .expect("capture registry lock poisoned");
        if streams.contains_key(&key) {
            return Err(CaptureError::InvalidConfig(
                "capture generation is already active",
            ));
        }
        let active = streams
            .values()
            .filter(|runtime| {
                matches!(
                    runtime.state(),
                    CaptureStreamState::Starting
                        | CaptureStreamState::Capturing
                        | CaptureStreamState::Hidden
                        | CaptureStreamState::Draining
                )
            })
            .count();
        if active >= coordinator.config.max_active_streams.get() {
            return Err(CaptureError::InvalidConfig("active stream limit reached"));
        }
    }
    // Install the new generation fence before subscriptions or task creation. A late callback
    // from the old attachment may still complete its acknowledgement, but it cannot allocate an
    // ordinal once this generation is accepted.
    if !coordinator.ordinals.begin_generation(&target) {
        return Err(CaptureError::InvalidConfig(
            "capture attachment generation is older than the active ordinal fence",
        ));
    }

    let scope = CommandScope::Session(target.transport_session.clone());
    let mut frames = transport.subscribe_named(&scope, FRAME_EVENT).await?;
    let mut visibility = transport.subscribe_named(&scope, VISIBILITY_EVENT).await?;
    let runtime = Arc::new(StreamRuntime::new(
        target,
        coordinator.config.clone(),
        coordinator.dependencies.clone(),
        Arc::clone(&coordinator.observer),
        Arc::clone(&transport),
        Arc::clone(&coordinator.ordinals),
    ));
    let (sender, receiver) = mpsc::channel(coordinator.config.queue_capacity.get());
    runtime.set_sender(sender.clone());

    let frame_runtime = Arc::clone(&runtime);
    let frame_transport = Arc::clone(&transport);
    let frame_task = tokio::spawn(async move {
        frame_reader(frame_runtime, frame_transport, &mut frames).await;
    });
    let visibility_runtime = Arc::clone(&runtime);
    let visibility_task = tokio::spawn(async move {
        visibility_reader(visibility_runtime, &mut visibility).await;
    });
    let worker_runtime = Arc::clone(&runtime);
    let worker_task = tokio::spawn(async move {
        worker_loop(worker_runtime, receiver).await;
    });
    runtime.set_tasks(frame_task, visibility_task, worker_task);

    let mut start_params = match runtime.config.format {
        ImageFormat::Jpeg => json!({
            "format": "jpeg",
            "quality": runtime.config.jpeg_quality.expect("validated JPEG quality"),
            "everyNthFrame": 1,
        }),
        ImageFormat::Png => json!({"format": "png", "everyNthFrame": 1}),
    };
    if let Some(maximum) = runtime.config.max_dimensions {
        let object = start_params
            .as_object_mut()
            .expect("screencast start parameters are an object");
        object.insert("maxWidth".into(), json!(maximum.width()));
        object.insert("maxHeight".into(), json!(maximum.height()));
    }
    if let Err(error) = transport.send_raw(&scope, START_METHOD, start_params).await {
        runtime.close_acceptance();
        runtime.abort_readers();
        if let Some(worker) = runtime.take_worker() {
            worker.abort();
        }
        return Err(error.into());
    }
    runtime.transition(Transition::StartedVisible);
    coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .insert(key, runtime);
    Ok(())
}

async fn frame_reader(
    runtime: Arc<StreamRuntime>,
    transport: Arc<dyn CdpTransport>,
    events: &mut Box<dyn TransportEvents>,
) {
    while runtime.accepting.load(Ordering::Acquire) {
        let event = match events.next().await {
            Ok(Some(event)) => event,
            Ok(None) | Err(_) => {
                runtime.fail();
                break;
            }
        };
        // The receipt sample marks the end of frame wait. Acknowledgement latency is measured
        // from this sample through successful ack completion; it includes token extraction but
        // excludes the preceding wait on the event stream and any downstream parse/handoff work.
        let observed = runtime.dependencies.clock.now();
        runtime.record_received();
        let Some(ack_token) = event.params.get("sessionId").and_then(Value::as_i64) else {
            runtime.declare_gap(
                CaptureGapReason::AcknowledgementFailed,
                runtime.session_time_for(observed),
                Some(1),
                Some("screencast frame acknowledgement token was invalid"),
            );
            runtime.fail();
            break;
        };
        let ack = time::timeout(
            runtime.config.ack_timeout,
            transport.send_raw(
                &CommandScope::Session(runtime.target.transport_session.clone()),
                ACK_METHOD,
                // The signed integer is an opaque CDP acknowledgement token. Keep it local and
                // echo it exactly; it is not a source sequence or continuity signal.
                json!({"sessionId": ack_token}),
            ),
        )
        .await;
        let ack_completed = runtime.dependencies.clock.now();
        let ack_failure_detail = match ack {
            Ok(Ok(_)) => None,
            Ok(Err(_)) => Some("screencast frame acknowledgement failed"),
            Err(_) => Some("screencast frame acknowledgement timed out"),
        };
        if let Some(detail) = ack_failure_detail {
            runtime.declare_gap(
                CaptureGapReason::AcknowledgementFailed,
                runtime.session_time_for(observed),
                Some(1),
                Some(detail),
            );
            runtime.fail();
            break;
        }
        let latency = ack_completed.as_nanos().saturating_sub(observed.as_nanos());
        let (observed_time, session_time) = runtime.record_ack(latency, observed);
        let ordinal = match runtime.ordinals.allocate(&runtime.target) {
            OrdinalAllocation::Allocated(ordinal) => ordinal,
            OrdinalAllocation::StaleGeneration => continue,
            OrdinalAllocation::Exhausted => {
                runtime.fail();
                break;
            }
        };
        runtime.transition(Transition::ActualFrame);
        let raw = match RawFrame::after_ack(
            event,
            ordinal,
            observed_time,
            session_time,
            runtime.config.format,
            runtime.config.max_base64_payload_bytes.get(),
        ) {
            Ok(raw) => raw,
            Err(_) => {
                runtime.dropped(CaptureGapReason::FrameRejected, session_time);
                continue;
            }
        };
        runtime.handoff(raw);
    }
}

async fn visibility_reader(runtime: Arc<StreamRuntime>, events: &mut Box<dyn TransportEvents>) {
    while runtime.accepting.load(Ordering::Acquire) {
        let event = match events.next().await {
            Ok(Some(event)) => event,
            Ok(None) | Err(_) => {
                runtime.fail();
                break;
            }
        };
        let visible = event.params.get("visible").and_then(Value::as_bool);
        match visible {
            Some(false) => {
                runtime.observer.visibility_changed(
                    runtime.target.target_id,
                    krometrail_core::TargetVisibility::Hidden,
                );
                if !runtime.transition(Transition::Hide) {
                    continue;
                }
                let at = runtime
                    .status()
                    .last_frame_session_time()
                    .unwrap_or(SessionTime::ZERO);
                runtime.declare_gap(
                    CaptureGapReason::TargetHidden,
                    at,
                    None,
                    Some("target hidden"),
                );
            }
            Some(true) => {
                runtime.observer.visibility_changed(
                    runtime.target.target_id,
                    krometrail_core::TargetVisibility::Visible,
                );
                runtime.transition(Transition::Show);
            }
            None => runtime.fail(),
        }
    }
}

async fn worker_loop(runtime: Arc<StreamRuntime>, mut receiver: mpsc::Receiver<RawFrame>) {
    while let Some(raw) = receiver.recv().await {
        runtime.begin_processing();
        if !persist_pending_gaps(&runtime).await {
            runtime.fail();
            break;
        }
        match decode_frame(&runtime, raw.clone()) {
            Ok(frame) => match runtime.dependencies.sink.append_frame(frame).await {
                Ok(_address) => {
                    runtime.persisted();
                    runtime.complete_processing();
                }
                Err(_) => {
                    runtime.complete_processing();
                    runtime.declare_gap(
                        CaptureGapReason::PersistenceRejected,
                        raw.session_time,
                        Some(1),
                        Some("frame persistence rejected"),
                    );
                    runtime.fail();
                    break;
                }
            },
            Err(_) => {
                runtime.complete_processing();
                if let Some(gap) = runtime.declare_gap(
                    CaptureGapReason::FrameRejected,
                    raw.session_time,
                    None,
                    Some("encoded frame rejected"),
                ) {
                    if runtime.dependencies.sink.append_gap(gap).await.is_err() {
                        runtime.fail();
                        break;
                    }
                }
            }
        }
    }
    if !persist_pending_gaps(&runtime).await {
        runtime.fail();
    }
}

async fn persist_pending_gaps(runtime: &StreamRuntime) -> bool {
    for gap in runtime.take_gaps() {
        if runtime.dependencies.sink.append_gap(gap).await.is_err() {
            return false;
        }
    }
    true
}

fn decode_frame(runtime: &StreamRuntime, raw: RawFrame) -> Result<EncodedFrame, ()> {
    if raw.data.is_empty() || raw.data.len() > runtime.config.max_base64_payload_bytes.get() {
        return Err(());
    }
    let bytes = STANDARD.decode(raw.data.as_bytes()).map_err(|_| ())?;
    let dimensions = super::image_header::dimensions(raw.format, &bytes).map_err(|_| ())?;
    if runtime
        .config
        .max_dimensions
        .is_some_and(|max| dimensions.width() > max.width() || dimensions.height() > max.height())
    {
        return Err(());
    }
    let frame_id = FrameId::from_uuid(*runtime.dependencies.ids.next().as_uuid());
    let metadata = CapturedFrame::new(
        frame_id,
        runtime.target.session_id,
        runtime.target.target_id,
        raw.capture_ordinal,
        raw.source_time,
        raw.observed_time,
        raw.session_time,
        raw.format,
        dimensions,
        raw.viewport,
        raw.device_scale_factor,
        raw.warnings,
    )
    .map_err(|_| ())?;
    EncodedFrame::new(metadata, bytes).map_err(|_| ())
}

pub(super) async fn stop_target(
    coordinator: &CaptureCoordinator,
    target: &CaptureTarget,
    reason: CaptureStopReason,
    deadline: Instant,
) -> CaptureStopOutcome {
    let key = StreamKey {
        target_id: target.target_id,
        attachment_generation: target.attachment_generation,
    };
    let runtime = coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .get(&key)
        .cloned();
    let Some(runtime) = runtime else {
        return CaptureStopOutcome {
            reason,
            complete: true,
            abandoned_accepted_frames: 0,
            emitted_gap_count: 0,
        };
    };
    if runtime.state() == CaptureStreamState::Stopped {
        return CaptureStopOutcome {
            reason,
            complete: true,
            abandoned_accepted_frames: 0,
            emitted_gap_count: 0,
        };
    }
    let before_gaps = runtime.status().statistics().gap_count();
    runtime.close_acceptance();
    runtime.transition(Transition::Stop);
    runtime.abort_readers();
    let scope = CommandScope::Session(runtime.target.transport_session.clone());
    let stop_succeeded = time::timeout_at(
        deadline,
        runtime.transport.send_raw(&scope, STOP_METHOD, json!({})),
    )
    .await
    .is_ok_and(|result| result.is_ok());

    let mut complete = stop_succeeded;
    let mut abandoned = 0;
    if let Some(mut worker) = runtime.take_worker() {
        tokio::select! {
            result = &mut worker => {
                complete &= result.is_ok();
                complete &= runtime.state() != CaptureStreamState::Failed;
            }
            _ = time::sleep_until(deadline) => {
                worker.abort();
                abandoned = runtime.abandon_queue();
                complete = false;
            }
        }
    }
    if !complete {
        abandoned = abandoned.max(runtime.abandon_queue());
        runtime.declare_gap(
            CaptureGapReason::CaptureStopped,
            runtime
                .status()
                .last_frame_session_time()
                .unwrap_or(SessionTime::ZERO),
            (abandoned > 0).then_some(abandoned),
            Some(if abandoned > 0 {
                "accepted frames abandoned at stop"
            } else {
                "capture stop deadline exhausted"
            }),
        );
    }
    runtime.transition(if complete {
        Transition::Drained
    } else {
        Transition::Deadline
    });
    let after_gaps = runtime.status().statistics().gap_count();
    // Remove only the exact stopped runtime. The StreamKey includes attachment_generation, so a
    // newer replacement has a different key and cannot be erased by this stop.
    {
        let mut streams = coordinator
            .streams
            .lock()
            .expect("capture registry lock poisoned");
        if let Some(existing) = streams.get(&key) {
            if Arc::ptr_eq(existing, &runtime) {
                streams.remove(&key);
            }
        }
    }
    // Terminal close/failure releases ordinal state; suspend, detach, and reconnect preserve it.
    match reason {
        CaptureStopReason::TargetClosed | CaptureStopReason::TargetFailed => {
            coordinator.ordinals.end_generation(target);
        }
        CaptureStopReason::TargetDetached
        | CaptureStopReason::SessionStopping
        | CaptureStopReason::Cancelled => {}
    }
    CaptureStopOutcome {
        reason,
        complete,
        abandoned_accepted_frames: abandoned,
        emitted_gap_count: after_gaps.saturating_sub(before_gaps),
    }
}

pub(super) async fn suspend_target(
    coordinator: &CaptureCoordinator,
    target: &CaptureTarget,
    at: SessionTime,
) {
    let key = StreamKey {
        target_id: target.target_id,
        attachment_generation: target.attachment_generation,
    };
    if let Some(runtime) = coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .get(&key)
        .cloned()
    {
        runtime.accepting.store(false, Ordering::Release);
        runtime.abort_readers();
        runtime.close_acceptance();
        runtime.transition(Transition::Suspend);
        runtime.declare_gap(
            CaptureGapReason::BrowserDisconnected,
            at,
            None,
            Some("transport suspended"),
        );
    }
}

pub(super) fn statuses(coordinator: &CaptureCoordinator) -> Vec<TargetCaptureStatus> {
    let statuses: Vec<_> = coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .values()
        .map(|runtime| runtime.status())
        .collect();
    // During generation replacement both the previous attachment and its replacement can briefly
    // coexist in the registry. Expose only the highest attachment generation per target.
    let mut best: std::collections::HashMap<krometrail_core::TargetId, TargetCaptureStatus> =
        std::collections::HashMap::new();
    for status in statuses {
        match best.get(&status.target_id()) {
            Some(existing)
                if existing.attachment_generation() >= status.attachment_generation() => {}
            _ => {
                best.insert(status.target_id(), status);
            }
        }
    }
    let mut statuses: Vec<_> = best.into_values().collect();
    statuses.sort_by_key(|status| status.target_id());
    statuses
}

pub(super) async fn shutdown(
    coordinator: &CaptureCoordinator,
    session_id: krometrail_core::SessionId,
    deadline: Instant,
) -> super::CaptureShutdownOutcome {
    let mut targets: Vec<_> = coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .values()
        .map(|runtime| runtime.target.clone())
        .collect();
    targets.sort_by_key(|target| (target.target_id, target.attachment_generation));
    let mut outcomes = Vec::with_capacity(targets.len());
    for target in targets {
        outcomes.push(
            stop_target(
                coordinator,
                &target,
                CaptureStopReason::SessionStopping,
                deadline,
            )
            .await,
        );
    }
    let flush_attempted = true;
    let flush_succeeded =
        time::timeout_at(deadline, coordinator.dependencies.sink.flush(session_id))
            .await
            .is_ok_and(|result| result.is_ok());
    coordinator.ordinals.clear();
    let complete = flush_succeeded && outcomes.iter().all(|outcome| outcome.complete);
    super::CaptureShutdownOutcome {
        targets: outcomes,
        flush_attempted,
        flush_succeeded,
        complete,
    }
}

fn status_from_state(
    target: &CaptureTarget,
    state: &RuntimeState,
) -> Result<TargetCaptureStatus, ()> {
    TargetCaptureStatus::new(
        target.target_id,
        target.attachment_generation,
        state.state,
        state.statistics,
        state.queue_capacity(),
        state.queue_depth,
        state.last_frame_session_time,
        state.ack_latency.summary(),
        state.frame_cadence.summary(),
    )
    .map_err(|_| ())
}

impl RuntimeState {
    fn queue_capacity(&self) -> usize {
        self.queue_capacity
    }
}

pub(super) fn next_state(
    current: CaptureStreamState,
    transition: Transition,
) -> Option<CaptureStreamState> {
    use CaptureStreamState::*;
    match (current, transition) {
        (Starting, Transition::StartedVisible) => Some(Capturing),
        (Starting, Transition::StartedHidden) => Some(Hidden),
        (Starting, Transition::Suspend) => Some(Suspended),
        (Starting, Transition::Stop) => Some(Draining),
        (Starting, Transition::Failure) => Some(Failed),
        (Capturing, Transition::Hide) => Some(Hidden),
        (Capturing, Transition::Suspend) => Some(Suspended),
        (Capturing, Transition::Stop) => Some(Draining),
        (Capturing, Transition::Failure) => Some(Failed),
        (Hidden, Transition::Show | Transition::ActualFrame) => Some(Capturing),
        (Hidden, Transition::Suspend) => Some(Suspended),
        (Hidden, Transition::Stop) => Some(Draining),
        (Hidden, Transition::Failure) => Some(Failed),
        (Suspended, Transition::Resume) => Some(Starting),
        (Suspended, Transition::Stop) => Some(Draining),
        (Suspended, Transition::Failure) => Some(Failed),
        (Draining, Transition::Failure) => Some(Failed),
        (Draining, Transition::Drained | Transition::Deadline) => Some(Stopped),
        _ => None,
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            buckets: [0; 64],
            sample_count: 0,
            max_nanos: None,
        }
    }
}

impl Histogram {
    pub(super) fn record(&mut self, nanos: u64) {
        let bucket = bucket_for(nanos);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
        self.sample_count = self.sample_count.saturating_add(1);
        self.max_nanos = Some(self.max_nanos.map_or(nanos, |max| max.max(nanos)));
    }

    pub(super) fn summary(&self) -> CaptureTimingSummary {
        if self.sample_count == 0 {
            return CaptureTimingSummary::empty();
        }
        CaptureTimingSummary::new(
            self.sample_count,
            self.nearest_rank(50),
            self.nearest_rank(95),
            self.nearest_rank(99),
            self.max_nanos,
        )
        .expect("histogram summaries are ordered")
    }

    fn nearest_rank(&self, percentile: u64) -> Option<u64> {
        let rank = self
            .sample_count
            .saturating_mul(percentile)
            .saturating_add(99)
            / 100;
        let mut seen = 0_u64;
        for (index, count) in self.buckets.iter().enumerate() {
            seen = seen.saturating_add(*count);
            if seen >= rank {
                return Some(bucket_upper_bound(index));
            }
        }
        self.max_nanos
    }
}

const fn bucket_for(value: u64) -> usize {
    if value == 0 {
        0
    } else {
        63 - value.leading_zeros() as usize
    }
}

const fn bucket_upper_bound(bucket: usize) -> u64 {
    match bucket {
        0 => 1,
        1..=62 => (1_u64 << (bucket + 1)) - 1,
        _ => u64::MAX,
    }
}

impl GapLedger {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            pending: VecDeque::with_capacity(capacity),
            saturation_open: false,
        }
    }

    pub(super) fn push(&mut self, gap: CaptureGap) -> CaptureGap {
        let is_saturation = *gap.reason() == CaptureGapReason::IngestionQueueSaturated;
        if is_saturation && self.saturation_open {
            if let Some(previous) = self.pending.back_mut() {
                if let Some(merged) = merge_gaps(previous, &gap) {
                    *previous = merged.clone();
                    return merged;
                }
            }
        }
        if self.pending.len() >= self.capacity {
            if self.capacity == 1 {
                if let Some(previous) = self.pending.pop_front() {
                    let merged = conservative_merge_gaps(&previous, &gap);
                    self.pending.push_back(merged.clone());
                    self.saturation_open = is_saturation;
                    return merged;
                }
            } else if let (Some(first), Some(second)) =
                (self.pending.pop_front(), self.pending.pop_front())
            {
                self.pending
                    .push_front(conservative_merge_gaps(&first, &second));
            }
        }
        self.pending.push_back(gap.clone());
        self.saturation_open = is_saturation;
        gap
    }

    fn close_saturation(&mut self) {
        self.saturation_open = false;
    }

    fn take(&mut self) -> Vec<CaptureGap> {
        self.saturation_open = false;
        self.pending.drain(..).collect()
    }
}

// When a bounded ledger is full, preserving an explicit broader gap is safer than dropping a
// reason entirely. The first reason remains the conservative classification and the count stays
// exact, while the detail makes the coalescing visible to downstream readers.
fn conservative_merge_gaps(first: &CaptureGap, second: &CaptureGap) -> CaptureGap {
    merge_gaps(first, second).unwrap_or_else(|| {
        let estimated = match (
            first.estimated_missing_frames(),
            second.estimated_missing_frames(),
        ) {
            (Some(left), Some(right)) => {
                std::num::NonZeroU64::new(left.get().saturating_add(right.get()))
            }
            _ => None,
        };
        CaptureGap::new(
            first.id(),
            first.session_id(),
            first.target_id(),
            SessionRange::new(
                first.range().start().min(second.range().start()),
                first.range().end().max(second.range().end()),
            )
            .expect("ordered coalesced range is valid"),
            *first.reason(),
            estimated,
            Some("coalesced bounded capture gap".to_owned()),
        )
        .expect("coalesced gap preserves core invariants")
    })
}

fn merge_gaps(first: &CaptureGap, second: &CaptureGap) -> Option<CaptureGap> {
    if first.reason() != second.reason()
        || first.session_id() != second.session_id()
        || first.target_id() != second.target_id()
    {
        return None;
    }
    let estimated = match (
        first.estimated_missing_frames(),
        second.estimated_missing_frames(),
    ) {
        (Some(left), Some(right)) => Some(std::num::NonZeroU64::new(
            left.get().checked_add(right.get())?,
        )?),
        _ => None,
    };
    CaptureGap::new(
        first.id(),
        first.session_id(),
        first.target_id(),
        SessionRange::new(
            first.range().start().min(second.range().start()),
            first.range().end().max(second.range().end()),
        )
        .ok()?,
        *first.reason(),
        estimated,
        first.detail().map(str::to_owned),
    )
    .ok()
}

impl RawFrame {
    fn after_ack(
        event: NamedEvent,
        capture_ordinal: CaptureOrdinal,
        observed_time: krometrail_core::ObservedTime,
        session_time: SessionTime,
        format: ImageFormat,
        max_payload_bytes: usize,
    ) -> Result<Self, ()> {
        let object = event.params.as_object().ok_or(())?;
        let data_value = object.get("data").and_then(Value::as_str).ok_or(())?;
        if data_value.len() > max_payload_bytes {
            return Err(());
        }
        let data = data_value.to_owned();
        let metadata = object
            .get("metadata")
            .and_then(Value::as_object)
            .ok_or(())?;
        let mut warnings = Vec::new();
        let source_time = match metadata.get("timestamp").and_then(Value::as_f64) {
            Some(value) if value.is_finite() && value >= 0.0 => {
                let scaled = value * 1_000_000_000.0;
                let nanos = scaled.round();
                if nanos.is_finite() && nanos >= i128::MIN as f64 && nanos <= i128::MAX as f64 {
                    if scaled.fract() == 0.0 {
                        Some(SourceTime::from_nanos(nanos as i128))
                    } else {
                        warnings.push(CaptureWarning::SourceTimestampRounded);
                        Some(SourceTime::from_nanos(nanos as i128))
                    }
                } else {
                    warnings.push(CaptureWarning::MissingSourceTime);
                    None
                }
            }
            _ => {
                warnings.push(CaptureWarning::MissingSourceTime);
                None
            }
        };
        let width = positive_u32(metadata.get("deviceWidth")).ok_or(())?;
        let height = positive_u32(metadata.get("deviceHeight")).ok_or(())?;
        let viewport = PixelDimensions::new(width, height).map_err(|_| ())?;
        let scale = match metadata.get("pageScaleFactor").and_then(Value::as_f64) {
            Some(value) => DeviceScaleFactor::new(value).map_err(|_| ())?,
            None => {
                warnings.push(CaptureWarning::ViewportMetadataIncomplete);
                DeviceScaleFactor::new(1.0).expect("one is a valid scale")
            }
        };
        Ok(Self {
            capture_ordinal,
            data,
            source_time,
            observed_time,
            session_time,
            format,
            viewport,
            device_scale_factor: scale,
            warnings,
        })
    }
}

fn positive_u32(value: Option<&Value>) -> Option<u32> {
    let value = value?.as_u64()?;
    u32::try_from(value).ok().filter(|value| *value > 0)
}

// Keep the transport future and sink future on the same bounded worker path. This helper exists
// solely to make the ordering test's blocked sink explicit without introducing another queue.
#[allow(dead_code)]
fn _assert_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StreamRuntime>();
}
