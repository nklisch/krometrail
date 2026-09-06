use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    CaptureFailure, CaptureFailureStage, CaptureGap, CaptureGapReason, CaptureOrdinal,
    CaptureStatistics, CaptureStreamState, CaptureTimingSummary, CaptureWarning, CapturedFrame,
    DeviceScaleFactor, EncodedFrame, ErrorCode, EveryNthFrame, FrameId, GapId, ImageFormat,
    KrometrailError, NonEmptyText, PixelDimensions, SessionRange, SessionTime, SourceTime,
    TargetCaptureStatus,
};
use serde_json::{Value, json};
use tokio::{
    sync::{Notify, mpsc},
    task::JoinHandle,
    time::{self, Instant},
};

use super::{
    CaptureCoordinator, CaptureDependencies, CaptureError, CaptureGeometry,
    CaptureGeometryTransition, CaptureObserver, CaptureStopOutcome, CaptureStopReason,
    CaptureTarget, StreamKey,
};
use crate::transport::{CdpTransport, CommandScope, NamedEvent, TransportEvents};

const FRAME_EVENT: &str = "Page.screencastFrame";
const VISIBILITY_EVENT: &str = "Page.screencastVisibilityChanged";
const GEOMETRY_EVENTS: &[&str] = &[
    "Page.frameResized",
    "Page.frameNavigated",
    "Page.navigatedWithinDocument",
];
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
        if let Some(state) = states.get(&key)
            && state.attachment_generation <= target.attachment_generation
        {
            states.remove(&key);
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
    every_nth_frame: EveryNthFrame,
    dependencies: CaptureDependencies,
    observer: Arc<dyn CaptureObserver>,
    transport: Arc<dyn CdpTransport>,
    accepting: AtomicBool,
    stop_notification: Notify,
    state: Mutex<RuntimeState>,
    control: Mutex<ControlHandles>,
    geometry: Mutex<GeometryAuthority>,
}

struct ControlHandles {
    sender: Option<mpsc::Sender<RawFrame>>,
    frame_reader: Option<JoinHandle<()>>,
    visibility_reader: Option<JoinHandle<()>>,
    geometry_readers: Vec<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug)]
struct GeometryAuthority {
    established: CaptureGeometry,
    revision: u64,
    transition: Option<GeometryTransitionState>,
}

#[derive(Clone, Copy, Debug)]
struct GeometryTransitionState {
    token: CaptureGeometryTransition,
}

#[derive(Clone, Copy, Debug)]
struct GeometryFence {
    revision: u64,
    geometry: CaptureGeometry,
    uncertain: bool,
}

#[derive(Clone, Copy, Debug)]
struct FrameGeometry {
    geometry: CaptureGeometry,
    metadata_uncertain: bool,
}

struct RuntimeState {
    state: CaptureStreamState,
    visible: bool,
    statistics: CaptureStatistics,
    queue_capacity: usize,
    queue_depth: usize,
    in_flight: usize,
    last_frame_session_time: Option<SessionTime>,
    previous_observed: Option<krometrail_core::ObservedTime>,
    ack_latency: Histogram,
    frame_cadence: Histogram,
    gaps: GapLedger,
    failure: Option<CaptureFailure>,
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
    PauseBudget,
    ResumeBudgetVisible,
    ResumeBudgetHidden,
    Stop,
    Drained,
    Deadline,
    Failure,
}

impl StreamRuntime {
    pub(super) fn new(
        target: CaptureTarget,
        config: super::CaptureConfig,
        every_nth_frame: EveryNthFrame,
        dependencies: CaptureDependencies,
        observer: Arc<dyn CaptureObserver>,
        transport: Arc<dyn CdpTransport>,
        ordinals: Arc<OrdinalRegistry>,
    ) -> Self {
        let geometry = target.geometry;
        Self {
            target,
            ordinals,
            config: config.clone(),
            every_nth_frame,
            dependencies,
            observer,
            transport,
            accepting: AtomicBool::new(true),
            stop_notification: Notify::new(),
            state: Mutex::new(RuntimeState {
                state: CaptureStreamState::Starting,
                visible: true,
                statistics: CaptureStatistics::default(),
                queue_capacity: config.queue_capacity.get(),
                queue_depth: 0,
                in_flight: 0,
                last_frame_session_time: None,
                previous_observed: None,
                ack_latency: Histogram::default(),
                frame_cadence: Histogram::default(),
                gaps: GapLedger::new(config.gap_ledger_capacity.get()),
                failure: None,
            }),
            control: Mutex::new(ControlHandles {
                sender: None,
                frame_reader: None,
                visibility_reader: None,
                geometry_readers: Vec::new(),
                worker: None,
            }),
            geometry: Mutex::new(GeometryAuthority {
                established: geometry,
                revision: 1,
                transition: None,
            }),
        }
    }

    fn geometry_fence(&self) -> GeometryFence {
        let authority = self
            .geometry
            .lock()
            .expect("capture geometry lock poisoned");
        GeometryFence {
            revision: authority.revision,
            geometry: authority.established,
            uncertain: authority.transition.is_some(),
        }
    }

    fn geometry_after_ack(&self, fence: GeometryFence) -> FrameGeometry {
        let authority = self
            .geometry
            .lock()
            .expect("capture geometry lock poisoned");
        let uncertain = fence.uncertain
            || authority.transition.is_some()
            || authority.revision != fence.revision;
        FrameGeometry {
            geometry: fence.geometry,
            metadata_uncertain: uncertain,
        }
    }

    fn begin_geometry_transition(&self) -> Option<(CaptureGeometryTransition, bool)> {
        let mut authority = self
            .geometry
            .lock()
            .expect("capture geometry lock poisoned");
        if let Some(transition) = authority.transition {
            return Some((transition.token, false));
        }
        let revision = authority.revision.checked_add(1)?;
        let token = CaptureGeometryTransition {
            target_id: self.target.target_id,
            attachment_generation: self.target.attachment_generation,
            revision,
        };
        authority.revision = revision;
        authority.transition = Some(GeometryTransitionState { token });
        Some((token, true))
    }

    fn finish_geometry_transition(
        &self,
        transition: CaptureGeometryTransition,
        geometry: Option<CaptureGeometry>,
    ) -> bool {
        let mut authority = self
            .geometry
            .lock()
            .expect("capture geometry lock poisoned");
        let Some(active) = authority.transition else {
            return false;
        };
        if active.token != transition {
            return false;
        }
        if let Some(geometry) = geometry {
            authority.established = geometry;
        }
        authority.transition = None;
        true
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
        geometry_readers: Vec<JoinHandle<()>>,
        worker: JoinHandle<()>,
    ) {
        let mut control = self.control.lock().expect("capture control lock poisoned");
        control.frame_reader = Some(frame_reader);
        control.visibility_reader = Some(visibility_reader);
        control.geometry_readers = geometry_readers;
        control.worker = Some(worker);
    }

    fn close_acceptance(&self) {
        self.accepting.store(false, Ordering::Release);
        self.stop_notification.notify_waiters();
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
        for handle in control.geometry_readers.drain(..) {
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
            match transition {
                Transition::Hide => state.visible = false,
                Transition::Show | Transition::ActualFrame => state.visible = true,
                _ => {}
            }
            let next = next_state(state.state, transition);
            let Some(next) = next else { return false };
            if next == state.state {
                return false;
            }
            state.state = next;
            status_from_state(&self.target, &state, self.every_nth_frame)
                .expect("runtime state maintains invariants")
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

    fn resume_budget_transition(&self) -> Transition {
        if self
            .state
            .lock()
            .expect("capture state lock poisoned")
            .visible
        {
            Transition::ResumeBudgetVisible
        } else {
            Transition::ResumeBudgetHidden
        }
    }

    async fn wait_until_recording_allowed(&self) -> bool {
        if !self.accepting.load(Ordering::Acquire) {
            return false;
        }
        let stopped = self.stop_notification.notified();
        tokio::pin!(stopped);
        stopped.as_mut().enable();
        if !self.accepting.load(Ordering::Acquire) {
            return false;
        }
        tokio::select! {
            result = self.dependencies.retention.wait_until_recording_allowed() => result.is_ok(),
            () = &mut stopped => false,
        }
    }

    pub(super) fn record_received(&self) {
        let mut state = self.state.lock().expect("capture state lock poisoned");
        state.statistics = state
            .statistics
            .record_received()
            .expect("capture counters cannot overflow in a bounded process");
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

    fn visibility_session_time(&self) -> krometrail_core::Result<SessionTime> {
        self.target
            .session_origin
            .normalize(self.dependencies.clock.now())
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
        state.statistics = state
            .statistics
            .record_acknowledged()
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
                state.statistics = state
                    .statistics
                    .record_accepted()
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
        self.dropped_with_detail(reason, at, None);
    }

    fn dropped_with_detail(
        &self,
        reason: CaptureGapReason,
        at: SessionTime,
        detail: Option<&'static str>,
    ) {
        let _ = self.declare_gap(reason, at, Some(1), detail);
        let mut state = self.state.lock().expect("capture state lock poisoned");
        state.statistics = state
            .statistics
            .record_dropped()
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
        state.statistics = state
            .statistics
            .record_persisted()
            .expect("persisted count cannot exceed accepted count");
    }

    fn declare_gap(
        &self,
        reason: CaptureGapReason,
        at: SessionTime,
        estimated: Option<u64>,
        detail: Option<&'static str>,
    ) -> Option<CaptureGap> {
        self.declare_gap_range(
            reason,
            SessionRange::new(at, at).expect("equal range is valid"),
            estimated,
            detail,
        )
    }

    fn declare_gap_range(
        &self,
        reason: CaptureGapReason,
        range: SessionRange,
        estimated: Option<u64>,
        detail: Option<&'static str>,
    ) -> Option<CaptureGap> {
        let gap = CaptureGap::new(
            GapId::from_uuid(*self.dependencies.ids.next().as_uuid()),
            self.target.session_id,
            self.target.target_id,
            range,
            self.dependencies.clock.now(),
            reason,
            estimated.and_then(std::num::NonZeroU64::new),
            detail.map(str::to_owned),
        )
        .ok()?;
        let notified = {
            let mut state = self.state.lock().expect("capture state lock poisoned");
            state.statistics = state
                .statistics
                .record_gap()
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
        status_from_state(&self.target, &state, self.every_nth_frame)
            .expect("runtime state maintains invariants")
    }

    fn fail(&self, failure: CaptureFailure) {
        self.accepting.store(false, Ordering::Release);
        let first_failure = {
            let mut state = self.state.lock().expect("capture state lock poisoned");
            record_first_failure(&mut state.failure, failure.clone())
        };
        if first_failure {
            let persistence = failure.cause().persistence.as_ref();
            tracing::error!(
                event = "capture.pipeline.failed",
                failure_stage = failure.stage().as_str(),
                cause_code = failure.cause().code.as_str(),
                persistence_operation = persistence.map(|value| value.operation().as_str()).unwrap_or("none"),
                persistence_category = persistence.map(|value| value.category().as_str()).unwrap_or("none"),
                persistence_recoverability = persistence.map(|value| value.recoverability().as_str()).unwrap_or("none"),
                session_id = %self.target.session_id,
                target_id = %self.target.target_id,
                attachment_generation = self.target.attachment_generation,
                "capture.pipeline.failed"
            );
        }
        let _ = self.transition(Transition::Failure);
        self.control
            .lock()
            .expect("capture control lock poisoned")
            .sender = None;
    }

    fn fail_at(&self, stage: CaptureFailureStage) {
        self.fail(
            CaptureFailure::new(
                stage,
                KrometrailError::new(
                    ErrorCode::CaptureFailed,
                    NonEmptyText::new("capture pipeline stage failed")
                        .expect("capture failure message is non-empty"),
                )
                .with_retry(krometrail_core::RetryAdvice::AfterRecovery),
            )
            .expect("capture failure cause is valid"),
        );
    }

    fn fail_acknowledgement(
        &self,
        reason: &'static str,
        observed: krometrail_core::ObservedTime,
        failed_at: krometrail_core::ObservedTime,
        detail: &'static str,
    ) {
        self.declare_gap(
            CaptureGapReason::AcknowledgementFailed,
            self.session_time_for(observed),
            Some(1),
            Some(detail),
        );
        let state = self.state.lock().expect("capture state lock poisoned");
        tracing::error!(
            event = "capture.ack.failed",
            reason,
            error_code = ErrorCode::CaptureFailed.as_str(),
            session_id = %self.target.session_id,
            target_id = %self.target.target_id,
            attachment_generation = self.target.attachment_generation,
            deadline_nanos = u64::try_from(self.config.ack_timeout.as_nanos()).unwrap_or(u64::MAX),
            elapsed_nanos = failed_at.as_nanos().saturating_sub(observed.as_nanos()),
            received_frames = state.statistics.received_frames(),
            acknowledged_frames = state.statistics.acknowledged_frames(),
            accepted_frames = state.statistics.accepted_frames(),
            dropped_frames = state.statistics.dropped_frames(),
            persisted_frames = state.statistics.persisted_frames(),
            gap_count = state.statistics.gap_count(),
            queue_depth = state.queue_depth,
            in_flight = state.in_flight,
            "capture.ack.failed"
        );
        drop(state);
        self.fail_at(CaptureFailureStage::Acknowledgement);
        self.observer
            .capture_stream_failed(self.target.connection_generation);
    }
}

pub(super) fn record_first_failure(
    current: &mut Option<CaptureFailure>,
    candidate: CaptureFailure,
) -> bool {
    if current.is_some() {
        false
    } else {
        *current = Some(candidate);
        true
    }
}

fn start_screencast_params(config: &super::CaptureConfig, every_nth_frame: EveryNthFrame) -> Value {
    let mut params = match config.format {
        ImageFormat::Jpeg => json!({
            "format": "jpeg",
            "quality": config.jpeg_quality.expect("validated JPEG quality"),
        }),
        ImageFormat::Png => json!({"format": "png"}),
    };
    let object = params
        .as_object_mut()
        .expect("screencast start parameters are an object");
    object.insert("everyNthFrame".into(), json!(every_nth_frame.get()));
    if let Some(maximum) = config.max_dimensions {
        object.insert("maxWidth".into(), json!(maximum.width()));
        object.insert("maxHeight".into(), json!(maximum.height()));
    }
    params
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
    // Admission counts registered streams and in-flight reservations together, and takes the
    // reservation in the same critical section. Checking `streams` alone would let every
    // concurrent start pass before any of them had inserted anything.
    let mut admission = {
        let streams = coordinator
            .streams
            .lock()
            .expect("capture registry lock poisoned");
        let mut pending = coordinator
            .pending_starts
            .lock()
            .expect("capture admission lock poisoned");
        if streams.contains_key(&key) || pending.contains(&key) {
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
                        | CaptureStreamState::PausedBudget
                        | CaptureStreamState::Hidden
                        | CaptureStreamState::Draining
                )
            })
            .count();
        if active + pending.len() >= coordinator.config.max_active_streams.get() {
            return Err(CaptureError::InvalidConfig("active stream limit reached"));
        }
        pending.insert(key);
        StreamAdmission {
            coordinator,
            key,
            committed: false,
        }
    };
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
    let mut geometry_events = Vec::with_capacity(GEOMETRY_EVENTS.len());
    for method in GEOMETRY_EVENTS {
        geometry_events.push(transport.subscribe_named(&scope, method).await?);
    }
    let runtime = Arc::new(StreamRuntime::new(
        target,
        coordinator.config.clone(),
        coordinator.every_nth_frame,
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
    let geometry_tasks = geometry_events
        .into_iter()
        .map(|mut events| {
            let geometry_runtime = Arc::clone(&runtime);
            tokio::spawn(async move {
                geometry_reader(geometry_runtime, &mut events).await;
            })
        })
        .collect();
    let worker_runtime = Arc::clone(&runtime);
    let worker_task = tokio::spawn(async move {
        worker_loop(worker_runtime, receiver).await;
    });
    runtime.set_tasks(frame_task, visibility_task, geometry_tasks, worker_task);

    let start_params = start_screencast_params(&runtime.config, runtime.every_nth_frame);
    if let Err(error) = transport.send_raw(&scope, START_METHOD, start_params).await {
        runtime.close_acceptance();
        runtime.abort_readers();
        if let Some(worker) = runtime.take_worker() {
            worker.abort();
        }
        return Err(error.into());
    }
    runtime.transition(Transition::StartedVisible);
    {
        let mut streams = coordinator
            .streams
            .lock()
            .expect("capture registry lock poisoned");
        // Hand the slot from the reservation to the registry without reopening the window: the
        // stream is counted by one side or the other at every instant.
        admission.commit();
        streams.insert(key, runtime);
    }
    Ok(())
}

/// Holds one slot against the active-stream cap from admission until the stream is registered.
/// Dropping it without `commit` releases the slot, so every early return and panic between the
/// two frees the reservation rather than permanently shrinking the cap.
struct StreamAdmission<'a> {
    coordinator: &'a CaptureCoordinator,
    key: StreamKey,
    committed: bool,
}

impl StreamAdmission<'_> {
    /// Called while the caller already holds the `streams` lock, so it must not take it again.
    fn commit(&mut self) {
        self.coordinator
            .pending_starts
            .lock()
            .expect("capture admission lock poisoned")
            .remove(&self.key);
        self.committed = true;
    }
}

impl Drop for StreamAdmission<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.coordinator
            .pending_starts
            .lock()
            .expect("capture admission lock poisoned")
            .remove(&self.key);
    }
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
                runtime.fail_at(CaptureFailureStage::FrameEventStream);
                runtime
                    .observer
                    .frame_event_stream_closed(runtime.target.connection_generation);
                break;
            }
        };
        // The receipt sample marks the end of frame wait. Acknowledgement latency is measured
        // from this sample through successful ack completion; it includes token extraction but
        // excludes the preceding wait on the event stream and any downstream parse/handoff work.
        let observed = runtime.dependencies.clock.now();
        let geometry_fence = runtime.geometry_fence();
        runtime.record_received();
        let Some(ack_token) = event.params.get("sessionId").and_then(Value::as_i64) else {
            runtime.fail_acknowledgement(
                "invalid_token",
                observed,
                runtime.dependencies.clock.now(),
                "screencast frame acknowledgement token was invalid",
            );
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
        let ack_failure = match ack {
            Ok(Ok(_)) => None,
            Ok(Err(_)) => Some(("transport_error", "screencast frame acknowledgement failed")),
            Err(_) => Some((
                "deadline_exceeded",
                "screencast frame acknowledgement timed out",
            )),
        };
        if let Some((reason, detail)) = ack_failure {
            runtime.fail_acknowledgement(reason, observed, ack_completed, detail);
            break;
        }
        let latency = ack_completed.as_nanos().saturating_sub(observed.as_nanos());
        let (observed_time, session_time) = runtime.record_ack(latency, observed);
        let ordinal = match runtime.ordinals.allocate(&runtime.target) {
            OrdinalAllocation::Allocated(ordinal) => ordinal,
            OrdinalAllocation::StaleGeneration => continue,
            OrdinalAllocation::Exhausted => {
                runtime.fail_at(CaptureFailureStage::OrdinalAllocation);
                break;
            }
        };
        runtime.transition(Transition::ActualFrame);
        let geometry = runtime.geometry_after_ack(geometry_fence);
        let raw = match RawFrame::after_ack(
            event,
            ordinal,
            observed_time,
            session_time,
            runtime.config.format,
            runtime.config.max_base64_payload_bytes.get(),
            geometry,
        ) {
            Ok(raw) => raw,
            Err(rejection_reason) => {
                tracing::warn!(
                    event = "capture.frame.rejected",
                    failure_stage = CaptureFailureStage::FrameEnvelope.as_str(),
                    error_code = ErrorCode::CaptureFailed.as_str(),
                    rejection_reason,
                    session_id = %runtime.target.session_id,
                    target_id = %runtime.target.target_id,
                    attachment_generation = runtime.target.attachment_generation,
                    "capture.frame.rejected"
                );
                runtime.dropped(CaptureGapReason::FrameRejected, session_time);
                runtime.fail_at(CaptureFailureStage::FrameEnvelope);
                break;
            }
        };
        runtime.handoff(raw);
    }
}

async fn geometry_reader(runtime: Arc<StreamRuntime>, events: &mut Box<dyn TransportEvents>) {
    while runtime.accepting.load(Ordering::Acquire) {
        match events.next().await {
            Ok(Some(_)) => {
                let Some((transition, _started)) = runtime.begin_geometry_transition() else {
                    runtime.fail_at(CaptureFailureStage::FrameEnvelope);
                    break;
                };
                if !runtime.observer.geometry_refresh_requested(transition) {
                    tracing::warn!(
                        event = "capture.geometry_refresh.dispatch_deferred",
                        target_id = %runtime.target.target_id,
                        attachment_generation = runtime.target.attachment_generation,
                        "capture.geometry_refresh.dispatch_deferred"
                    );
                }
            }
            Ok(None) | Err(_) => {
                let transition = runtime.begin_geometry_transition().map(|value| value.0);
                if let Some(transition) = transition {
                    runtime.finish_geometry_transition(transition, None);
                }
                runtime.fail_at(CaptureFailureStage::FrameEventStream);
                break;
            }
        }
    }
}

async fn visibility_reader(runtime: Arc<StreamRuntime>, events: &mut Box<dyn TransportEvents>) {
    while runtime.accepting.load(Ordering::Acquire) {
        let event = match events.next().await {
            Ok(Some(event)) => event,
            Ok(None) | Err(_) => {
                runtime.fail_at(CaptureFailureStage::VisibilityEventStream);
                break;
            }
        };
        let observed_at = match runtime.visibility_session_time() {
            Ok(observed_at) => observed_at,
            Err(_) => {
                tracing::warn!(
                    event = "capture.visibility.dropped",
                    target_id = %runtime.target.target_id,
                    event_name = VISIBILITY_EVENT,
                    "capture.visibility.dropped"
                );
                continue;
            }
        };
        let visible = event.params.get("visible").and_then(Value::as_bool);
        match visible {
            Some(false) => {
                runtime.observer.visibility_changed(
                    runtime.target.target_id,
                    krometrail_core::TargetVisibility::Hidden,
                    observed_at,
                );
                if !runtime.transition(Transition::Hide) {
                    continue;
                }
                runtime.declare_gap(
                    CaptureGapReason::TargetHidden,
                    observed_at,
                    None,
                    Some("target hidden"),
                );
            }
            Some(true) => {
                runtime.observer.visibility_changed(
                    runtime.target.target_id,
                    krometrail_core::TargetVisibility::Visible,
                    observed_at,
                );
                runtime.transition(Transition::Show);
            }
            None => {
                runtime.fail_at(CaptureFailureStage::VisibilityEventStream);
            }
        }
    }
}

async fn worker_loop(runtime: Arc<StreamRuntime>, mut receiver: mpsc::Receiver<RawFrame>) {
    while let Some(raw) = receiver.recv().await {
        runtime.begin_processing();
        if let Err(error) = persist_pending_gaps(&runtime).await {
            runtime.fail(
                CaptureFailure::new(CaptureFailureStage::GapPersistence, error)
                    .expect("persistence errors are valid capture causes"),
            );
            break;
        }
        match decode_frame(&runtime, raw.clone()) {
            Ok(frame) => match runtime.dependencies.sink.append_frame(frame).await {
                Ok(_address) => {
                    runtime.persisted();
                    runtime.complete_processing();
                }
                Err(error) if error.code == ErrorCode::BudgetExhausted => {
                    runtime.complete_processing();
                    runtime.declare_gap(
                        CaptureGapReason::PersistenceRejected,
                        raw.session_time,
                        Some(1),
                        Some("disk budget paused capture"),
                    );
                    runtime.transition(Transition::PauseBudget);
                    if !runtime.wait_until_recording_allowed().await {
                        break;
                    }
                    if let Err(error) = persist_pending_gaps(&runtime).await {
                        runtime.fail(
                            CaptureFailure::new(CaptureFailureStage::GapPersistence, error)
                                .expect("persistence errors are valid capture causes"),
                        );
                        break;
                    }
                    runtime.transition(runtime.resume_budget_transition());
                }
                Err(error) => {
                    runtime.complete_processing();
                    runtime.declare_gap(
                        CaptureGapReason::PersistenceRejected,
                        raw.session_time,
                        Some(1),
                        Some("frame persistence rejected"),
                    );
                    runtime.fail(
                        CaptureFailure::new(CaptureFailureStage::FramePersistence, error)
                            .expect("persistence errors are valid capture causes"),
                    );
                    break;
                }
            },
            Err(_) => {
                runtime.complete_processing();
                // Reader-side and worker-side rejection both discard exactly one frame, so both
                // report a count of one. Leaving this side uncounted made an identical loss look
                // like an unquantified gap purely because of where the rejection was detected.
                if let Some(gap) = runtime.declare_gap(
                    CaptureGapReason::FrameRejected,
                    raw.session_time,
                    Some(1),
                    Some("encoded frame rejected"),
                ) && runtime.dependencies.sink.append_gap(gap).await.is_err()
                {
                    runtime.fail_at(CaptureFailureStage::GapPersistence);
                    break;
                }
                runtime.fail_at(CaptureFailureStage::FrameDecode);
                break;
            }
        }
    }
    if let Err(error) = persist_pending_gaps(&runtime).await {
        runtime.fail(
            CaptureFailure::new(CaptureFailureStage::GapPersistence, error)
                .expect("persistence errors are valid capture causes"),
        );
    }
}

async fn persist_pending_gaps(runtime: &StreamRuntime) -> krometrail_core::Result<()> {
    for gap in runtime.take_gaps() {
        runtime.dependencies.sink.append_gap(gap).await?;
    }
    Ok(())
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
            complete: true,
            abandoned_accepted_frames: 0,
            capture_failure: None,
        };
    };
    if runtime.state() == CaptureStreamState::Stopped {
        return CaptureStopOutcome {
            complete: true,
            abandoned_accepted_frames: 0,
            capture_failure: runtime.status().failure().cloned(),
        };
    }
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
    let capture_failure = runtime.status().failure().cloned();
    // Remove only the exact stopped runtime. The StreamKey includes attachment_generation, so a
    // newer replacement has a different key and cannot be erased by this stop.
    {
        let mut streams = coordinator
            .streams
            .lock()
            .expect("capture registry lock poisoned");
        if let Some(existing) = streams.get(&key)
            && Arc::ptr_eq(existing, &runtime)
        {
            streams.remove(&key);
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
        complete,
        abandoned_accepted_frames: abandoned,
        capture_failure,
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
    let mut capture_failure = statuses(coordinator)
        .into_iter()
        .find_map(|status| status.failure().cloned());
    let mut targets: Vec<_> = coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .values()
        .map(|runtime| runtime.target.clone())
        .collect();
    targets.sort_by_key(|target| (target.target_id, target.attachment_generation));
    let mut targets_complete = true;
    for target in targets {
        let outcome = stop_target(
            coordinator,
            &target,
            CaptureStopReason::SessionStopping,
            deadline,
        )
        .await;
        if capture_failure.is_none() {
            capture_failure = outcome.capture_failure;
        }
        targets_complete &= outcome.complete;
    }
    let flush_attempted = true;
    let flush_result =
        time::timeout_at(deadline, coordinator.dependencies.sink.flush(session_id)).await;
    let flush_succeeded = matches!(&flush_result, Ok(Ok(())));
    if capture_failure.is_none() {
        capture_failure = match flush_result {
            Ok(Err(error)) => Some(
                CaptureFailure::new(CaptureFailureStage::FramePersistence, error).unwrap_or_else(
                    |_| {
                        CaptureFailure::new(
                            CaptureFailureStage::FramePersistence,
                            KrometrailError::new(
                                ErrorCode::CaptureFailed,
                                NonEmptyText::new("capture session flush failed")
                                    .expect("capture failure message is non-empty"),
                            ),
                        )
                        .expect("capture failure cause is valid")
                    },
                ),
            ),
            Err(_) => Some(
                CaptureFailure::new(
                    CaptureFailureStage::FramePersistence,
                    KrometrailError::new(
                        ErrorCode::CaptureFailed,
                        NonEmptyText::new("capture session flush deadline expired")
                            .expect("capture failure message is non-empty"),
                    ),
                )
                .expect("capture failure cause is valid"),
            ),
            Ok(Ok(())) => None,
        };
    }
    coordinator.ordinals.clear();
    let complete = flush_succeeded && targets_complete;
    super::CaptureShutdownOutcome {
        flush_attempted,
        flush_succeeded,
        complete,
        capture_failure,
    }
}

fn status_from_state(
    target: &CaptureTarget,
    state: &RuntimeState,
    every_nth_frame: EveryNthFrame,
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
        every_nth_frame,
        state.failure.clone(),
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
        (Capturing, Transition::PauseBudget) => Some(PausedBudget),
        (Capturing, Transition::Suspend) => Some(Suspended),
        (Capturing, Transition::Stop) => Some(Draining),
        (Capturing, Transition::Failure) => Some(Failed),
        (PausedBudget, Transition::Hide | Transition::Show | Transition::ActualFrame) => {
            Some(PausedBudget)
        }
        (PausedBudget, Transition::ResumeBudgetVisible) => Some(Capturing),
        (PausedBudget, Transition::ResumeBudgetHidden) => Some(Hidden),
        (PausedBudget, Transition::Suspend) => Some(Suspended),
        (PausedBudget, Transition::Stop) => Some(Draining),
        (PausedBudget, Transition::Failure) => Some(Failed),
        (Hidden, Transition::Show | Transition::ActualFrame) => Some(Capturing),
        (Hidden, Transition::PauseBudget) => Some(PausedBudget),
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
        if is_saturation
            && self.saturation_open
            && let Some(previous) = self.pending.back_mut()
            && let Some(merged) = merge_gaps(previous, &gap)
        {
            *previous = merged.clone();
            return merged;
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

/// Sums the counts that are known. Only some gap reasons count discrete frames at all — a hidden
/// target or a stopped capture spans time without any countable dropped frame — so an absent count
/// means "contributes nothing", not "unknown total". Treating a mixed merge as unknown would throw
/// away the one hard number in the pair and under-report loss to the agent reading the evidence.
fn aggregate_estimated_frames(
    first: Option<std::num::NonZeroU64>,
    second: Option<std::num::NonZeroU64>,
) -> Option<std::num::NonZeroU64> {
    match (first, second) {
        (Some(left), Some(right)) => {
            std::num::NonZeroU64::new(left.get().saturating_add(right.get()))
        }
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

// When a bounded ledger is full, preserving an explicit broader gap is safer than dropping a
// reason entirely. The first reason remains the conservative classification and every known count
// is carried forward, while the detail makes the coalescing visible to downstream readers.
fn conservative_merge_gaps(first: &CaptureGap, second: &CaptureGap) -> CaptureGap {
    merge_gaps(first, second).unwrap_or_else(|| {
        let estimated = aggregate_estimated_frames(
            first.estimated_missing_frames(),
            second.estimated_missing_frames(),
        );
        CaptureGap::new(
            first.id(),
            first.session_id(),
            first.target_id(),
            SessionRange::new(
                first.range().start().min(second.range().start()),
                first.range().end().max(second.range().end()),
            )
            .expect("ordered coalesced range is valid"),
            first.observed_time().max(second.observed_time()),
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
    let estimated = aggregate_estimated_frames(
        first.estimated_missing_frames(),
        second.estimated_missing_frames(),
    );
    CaptureGap::new(
        first.id(),
        first.session_id(),
        first.target_id(),
        SessionRange::new(
            first.range().start().min(second.range().start()),
            first.range().end().max(second.range().end()),
        )
        .ok()?,
        first.observed_time().max(second.observed_time()),
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
        geometry: FrameGeometry,
    ) -> Result<Self, &'static str> {
        let object = event.params.as_object().ok_or("params_not_object")?;
        let data_value = object
            .get("data")
            .and_then(Value::as_str)
            .ok_or("data_missing_or_not_string")?;
        if data_value.len() > max_payload_bytes {
            return Err("payload_exceeds_limit");
        }
        let data = data_value.to_owned();
        let metadata = object
            .get("metadata")
            .and_then(Value::as_object)
            .ok_or("metadata_missing_or_not_object")?;
        let mut warnings = Vec::new();
        if geometry.metadata_uncertain {
            warnings.push(CaptureWarning::ViewportMetadataIncomplete);
        }
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
        Ok(Self {
            capture_ordinal,
            data,
            source_time,
            observed_time,
            session_time,
            format,
            viewport: geometry.geometry.viewport,
            device_scale_factor: geometry.geometry.device_scale_factor,
            warnings,
        })
    }
}

fn runtime_for_transition(
    coordinator: &CaptureCoordinator,
    target_id: krometrail_core::TargetId,
    attachment_generation: u64,
) -> Option<Arc<StreamRuntime>> {
    coordinator
        .streams
        .lock()
        .expect("capture registry lock poisoned")
        .get(&StreamKey {
            target_id,
            attachment_generation,
        })
        .cloned()
}

pub(super) fn begin_geometry_transition(
    coordinator: &CaptureCoordinator,
    target_id: krometrail_core::TargetId,
    attachment_generation: u64,
) -> Option<(CaptureGeometryTransition, bool)> {
    runtime_for_transition(coordinator, target_id, attachment_generation)?
        .begin_geometry_transition()
}

pub(super) fn commit_geometry_transition(
    coordinator: &CaptureCoordinator,
    transition: CaptureGeometryTransition,
    geometry: CaptureGeometry,
) -> bool {
    runtime_for_transition(
        coordinator,
        transition.target_id,
        transition.attachment_generation,
    )
    .is_some_and(|runtime| runtime.finish_geometry_transition(transition, Some(geometry)))
}

#[cfg(test)]
pub(super) fn geometry_for_test(
    coordinator: &CaptureCoordinator,
    target_id: krometrail_core::TargetId,
    attachment_generation: u64,
) -> Option<(CaptureGeometry, bool)> {
    let runtime = runtime_for_transition(coordinator, target_id, attachment_generation)?;
    let authority = runtime
        .geometry
        .lock()
        .expect("capture geometry lock poisoned");
    Some((authority.established, authority.transition.is_some()))
}
