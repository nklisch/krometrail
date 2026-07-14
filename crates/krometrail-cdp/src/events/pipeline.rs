use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use krometrail_core::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventCollectionGap,
    BrowserEventCollectionState, BrowserEventCollectionStatus, BrowserEventGapReason,
    BrowserEventId, BrowserEventOrdinal, BrowserEventPayload, BrowserEventSeverity,
    BrowserEventSink, BrowserSourceTimestamp, IdSource, MonotonicClock, ObservedTime, SessionId,
    SessionOrigin, SessionRange, SessionTime, TargetId,
};
use tokio::{sync::mpsc, task::JoinHandle};

use super::{BrowserEventConfig, status::BrowserEventStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SubmitOutcome {
    Accepted,
    Dropped,
    StaleGeneration,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TargetGeneration {
    pub(super) connection: u64,
    pub(super) attachment: u64,
}

#[derive(Clone)]
pub(super) struct EventPipeline {
    inner: Arc<PipelineInner>,
}

struct PipelineInner {
    session_id: SessionId,
    session_origin: SessionOrigin,
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    sink: Option<Arc<dyn BrowserEventSink>>,
    config: BrowserEventConfig,
    targets: Mutex<HashMap<TargetId, Arc<TargetPipeline>>>,
    pending_bytes: AtomicUsize,
    dropped_count: AtomicU64,
    persisted_count: AtomicU64,
    state: Mutex<CollectionState>,
}

struct CollectionState {
    status: BrowserEventCollectionStatus,
    unavailable: BTreeSet<BrowserEventClass>,
}

struct TargetPipeline {
    ingress: Arc<TargetIngress>,
    writer: Mutex<Option<JoinHandle<bool>>>,
}

pub(super) struct TargetIngress {
    target_id: TargetId,
    inner: Arc<PipelineInner>,
    sender: Option<mpsc::Sender<QueuedEvent>>,
    state: Mutex<TargetIngressState>,
    wake_writer: tokio::sync::Notify,
    stop_writer: tokio::sync::Notify,
    stopping: AtomicBool,
}

struct TargetIngressState {
    generation: TargetGeneration,
    accepting: bool,
    last_ordinal: u64,
    last_session_time: SessionTime,
    gaps: GapLedger,
}

struct QueuedEvent {
    event: BrowserEvent,
    bytes: usize,
}

#[derive(Clone)]
struct GapEntry {
    generation: TargetGeneration,
    reason: BrowserEventGapReason,
    class: Option<BrowserEventClass>,
    range: SessionRange,
    first_ordinal: BrowserEventOrdinal,
    last_ordinal: BrowserEventOrdinal,
    count: u64,
    ledger_merged: bool,
}

struct GapLedger {
    capacity: usize,
    entries: VecDeque<GapEntry>,
}

impl EventPipeline {
    pub(super) fn new(
        session_id: SessionId,
        session_origin: SessionOrigin,
        clock: Arc<dyn MonotonicClock>,
        ids: Arc<dyn IdSource>,
        sink: Option<Arc<dyn BrowserEventSink>>,
        config: BrowserEventConfig,
    ) -> krometrail_core::Result<Self> {
        config.validate()?;
        let status = if config.enabled {
            BrowserEventCollectionStatus::Starting
        } else {
            BrowserEventCollectionStatus::Disabled
        };
        Ok(Self {
            inner: Arc::new(PipelineInner {
                session_id,
                session_origin,
                clock,
                ids,
                sink,
                config,
                targets: Mutex::new(HashMap::new()),
                pending_bytes: AtomicUsize::new(0),
                dropped_count: AtomicU64::new(0),
                persisted_count: AtomicU64::new(0),
                state: Mutex::new(CollectionState {
                    status,
                    unavailable: BTreeSet::new(),
                }),
            }),
        })
    }

    pub(super) fn semantic_enabled(&self) -> bool {
        self.inner.config.enabled && self.inner.sink.is_some()
    }

    pub(super) fn has_sink(&self) -> bool {
        self.inner.sink.is_some()
    }

    pub(super) fn ids(&self) -> Arc<dyn IdSource> {
        Arc::clone(&self.inner.ids)
    }

    pub(super) fn begin_target(
        &self,
        target_id: TargetId,
        generation: TargetGeneration,
    ) -> Result<Arc<TargetIngress>, ()> {
        if generation.attachment == 0 {
            return Err(());
        }
        let mut targets = self
            .inner
            .targets
            .lock()
            .expect("event target registry lock");
        if let Some(existing) = targets.get(&target_id) {
            existing.ingress.begin_generation(generation)?;
            return Ok(Arc::clone(&existing.ingress));
        }
        if targets.len() >= self.inner.config.max_active_targets.get() {
            return Err(());
        }
        let (sender, receiver) = if self.inner.sink.is_some() {
            let (sender, receiver) =
                mpsc::channel(self.inner.config.per_target_queue_capacity.get());
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        let ingress = Arc::new(TargetIngress {
            target_id,
            inner: Arc::clone(&self.inner),
            sender,
            state: Mutex::new(TargetIngressState {
                generation,
                accepting: true,
                last_ordinal: 0,
                last_session_time: SessionTime::ZERO,
                gaps: GapLedger::new(self.inner.config.gap_ledger_capacity.get()),
            }),
            wake_writer: tokio::sync::Notify::new(),
            stop_writer: tokio::sync::Notify::new(),
            stopping: AtomicBool::new(false),
        });
        let writer = receiver.map(|receiver| {
            let ingress = Arc::clone(&ingress);
            tokio::spawn(async move { writer_loop(ingress, receiver).await })
        });
        targets.insert(
            target_id,
            Arc::new(TargetPipeline {
                ingress: Arc::clone(&ingress),
                writer: Mutex::new(writer),
            }),
        );
        Ok(ingress)
    }

    pub(super) fn mark_operational(&self) {
        let mut state = self.inner.state.lock().expect("event status lock");
        if self.inner.config.enabled && state.unavailable.is_empty() {
            state.status = BrowserEventCollectionStatus::Operational;
        }
    }

    pub(super) fn mark_degraded(&self, class: BrowserEventClass) {
        let mut state = self.inner.state.lock().expect("event status lock");
        state.unavailable.insert(class);
        if self.inner.config.enabled {
            state.status = BrowserEventCollectionStatus::Degraded;
        }
    }

    pub(super) fn mark_suspended(&self) {
        let mut state = self.inner.state.lock().expect("event status lock");
        if self.inner.config.enabled {
            state.status = BrowserEventCollectionStatus::Suspended;
        }
    }

    pub(super) fn status(&self) -> BrowserEventStatus {
        let state = self.inner.state.lock().expect("event status lock");
        let pending_gap_count = self
            .inner
            .targets
            .lock()
            .expect("event target registry lock")
            .values()
            .map(|target| target.ingress.pending_gap_count())
            .sum();
        BrowserEventStatus {
            state: state.status,
            unavailable_classes: state.unavailable.iter().copied().collect(),
            dropped_count: self.inner.dropped_count.load(Ordering::Acquire),
            persisted_count: self.inner.persisted_count.load(Ordering::Acquire),
            pending_bytes: self.inner.pending_bytes.load(Ordering::Acquire),
            pending_gap_count,
        }
    }

    pub(super) fn collection_state_payload(&self) -> Option<BrowserEventPayload> {
        let status = self.status();
        BrowserEventCollectionState::new(
            status.state,
            status.unavailable_classes,
            status.dropped_count,
            status.persisted_count,
        )
        .ok()
        .map(BrowserEventPayload::CollectionStateChanged)
    }

    pub(super) async fn stop_target(
        &self,
        target_id: TargetId,
        generation: TargetGeneration,
        deadline: tokio::time::Instant,
    ) -> bool {
        let target = self
            .inner
            .targets
            .lock()
            .expect("event target registry lock")
            .get(&target_id)
            .cloned();
        let Some(target) = target else {
            return true;
        };
        if target.ingress.generation() != generation {
            return true;
        }
        target.ingress.close();
        let writer = target.writer.lock().expect("event writer lock").take();
        let complete = match writer {
            Some(mut writer) => match tokio::time::timeout_at(deadline, &mut writer).await {
                Ok(result) => result.unwrap_or(false),
                Err(_) => {
                    // Dropping a JoinHandle detaches it. Explicit abort is required so a
                    // wedged sink cannot outlive the aggregate shutdown boundary.
                    writer.abort();
                    false
                }
            },
            None => target.ingress.pending_gap_count() == 0,
        };
        self.inner
            .targets
            .lock()
            .expect("event target registry lock")
            .remove(&target_id);
        complete
    }

    pub(super) async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        let targets: Vec<_> = self
            .inner
            .targets
            .lock()
            .expect("event target registry lock")
            .iter()
            .map(|(target_id, target)| (*target_id, target.ingress.generation()))
            .collect();
        let mut complete = true;
        for (target_id, generation) in targets {
            complete &= self.stop_target(target_id, generation, deadline).await;
        }
        let mut state = self.inner.state.lock().expect("event status lock");
        state.status = if complete {
            BrowserEventCollectionStatus::Stopped
        } else {
            BrowserEventCollectionStatus::Failed
        };
        complete
    }
}

impl TargetIngress {
    fn begin_generation(&self, generation: TargetGeneration) -> Result<(), ()> {
        let mut state = self.state.lock().expect("event ingress state lock");
        let stale = generation.connection < state.generation.connection
            || (generation.connection == state.generation.connection
                && (generation.attachment < state.generation.attachment
                    || (generation.attachment == state.generation.attachment && state.accepting)));
        if stale {
            return Err(());
        }
        state.generation = generation;
        state.accepting = true;
        self.stopping.store(false, Ordering::Release);
        Ok(())
    }

    pub(super) fn generation(&self) -> TargetGeneration {
        self.state
            .lock()
            .expect("event ingress state lock")
            .generation
    }

    pub(super) fn suspend_generation(
        &self,
        generation: TargetGeneration,
        reason: BrowserEventGapReason,
    ) {
        let observed = self.inner.clock.now();
        let mut state = self.state.lock().expect("event ingress state lock");
        if state.generation != generation || !state.accepting {
            return;
        }
        state.accepting = false;
        if self.sender.is_none() {
            return;
        }
        if let Some((ordinal, at)) = allocate(&mut state, self.inner.session_origin, observed) {
            push_gap(
                &mut state,
                generation,
                reason,
                Some(BrowserEventClass::Operational),
                at,
                ordinal,
            );
            self.inner.dropped_count.fetch_add(1, Ordering::AcqRel);
            self.wake_writer.notify_one();
        }
    }

    pub(super) fn submit_payload(
        &self,
        generation: TargetGeneration,
        observed: ObservedTime,
        source_time: Option<BrowserSourceTimestamp>,
        payload: BrowserEventPayload,
    ) -> SubmitOutcome {
        let (ordinal, session_time) = {
            let mut state = self.state.lock().expect("event ingress state lock");
            if !state.accepting || state.generation != generation {
                return SubmitOutcome::StaleGeneration;
            }
            let Some(allocation) = allocate(&mut state, self.inner.session_origin, observed) else {
                return SubmitOutcome::Dropped;
            };
            allocation
        };
        let class = payload.class();
        let Some(event) = build_event(
            self.inner.as_ref(),
            self.target_id,
            generation,
            ordinal,
            session_time,
            source_time,
            observed,
            payload,
        ) else {
            self.record_allocated_drop(
                generation,
                BrowserEventGapReason::InvalidPayload,
                Some(class),
                session_time,
                ordinal,
            );
            return SubmitOutcome::Dropped;
        };
        let Some(sender) = self.sender.as_ref() else {
            return SubmitOutcome::Disabled;
        };
        let bytes = match serde_json::to_vec(&event) {
            Ok(value) => value.len(),
            Err(_) => {
                self.record_allocated_drop(
                    generation,
                    BrowserEventGapReason::InvalidPayload,
                    Some(class),
                    session_time,
                    ordinal,
                );
                return SubmitOutcome::Dropped;
            }
        };
        if !reserve_pending(&self.inner, bytes) {
            self.record_allocated_drop(
                generation,
                BrowserEventGapReason::QueueSaturated,
                Some(class),
                session_time,
                ordinal,
            );
            return SubmitOutcome::Dropped;
        }
        match sender.try_send(QueuedEvent { event, bytes }) {
            Ok(()) => SubmitOutcome::Accepted,
            Err(mpsc::error::TrySendError::Full(queued)) => {
                release_pending(&self.inner, queued.bytes);
                self.record_allocated_drop(
                    generation,
                    BrowserEventGapReason::QueueSaturated,
                    Some(class),
                    session_time,
                    ordinal,
                );
                SubmitOutcome::Dropped
            }
            Err(mpsc::error::TrySendError::Closed(queued)) => {
                release_pending(&self.inner, queued.bytes);
                self.record_allocated_drop(
                    generation,
                    BrowserEventGapReason::SubscriptionClosed,
                    Some(class),
                    session_time,
                    ordinal,
                );
                SubmitOutcome::Dropped
            }
        }
    }

    pub(super) fn record_observed_drop(
        &self,
        generation: TargetGeneration,
        reason: BrowserEventGapReason,
        class: Option<BrowserEventClass>,
        observed: ObservedTime,
    ) -> SubmitOutcome {
        self.record_observed_drops(generation, reason, class, observed, 1)
    }

    pub(super) fn record_observed_drops(
        &self,
        generation: TargetGeneration,
        reason: BrowserEventGapReason,
        class: Option<BrowserEventClass>,
        observed: ObservedTime,
        count: u64,
    ) -> SubmitOutcome {
        if self.sender.is_none() {
            return SubmitOutcome::Disabled;
        }
        let mut state = self.state.lock().expect("event ingress state lock");
        if !state.accepting || state.generation != generation {
            return SubmitOutcome::StaleGeneration;
        }
        let mut recorded = 0_u64;
        for _ in 0..count {
            let Some((ordinal, at)) = allocate(&mut state, self.inner.session_origin, observed)
            else {
                break;
            };
            push_gap(&mut state, generation, reason, class, at, ordinal);
            recorded = recorded.saturating_add(1);
        }
        self.inner
            .dropped_count
            .fetch_add(recorded, Ordering::AcqRel);
        drop(state);
        self.wake_writer.notify_one();
        SubmitOutcome::Dropped
    }

    fn record_allocated_drop(
        &self,
        generation: TargetGeneration,
        reason: BrowserEventGapReason,
        class: Option<BrowserEventClass>,
        at: SessionTime,
        ordinal: BrowserEventOrdinal,
    ) {
        let mut state = self.state.lock().expect("event ingress state lock");
        push_gap(&mut state, generation, reason, class, at, ordinal);
        self.inner.dropped_count.fetch_add(1, Ordering::AcqRel);
        drop(state);
        self.wake_writer.notify_one();
    }

    fn record_batch_failure(&self, events: &[QueuedEvent]) {
        let Some(first) = events.first() else { return };
        let last = events.last().expect("non-empty batch has last event");
        let class = events
            .iter()
            .map(|event| event.event.class())
            .all(|class| class == first.event.class())
            .then(|| first.event.class());
        let entry = GapEntry {
            generation: TargetGeneration {
                connection: 0,
                attachment: first.event.attachment_generation(),
            },
            reason: BrowserEventGapReason::PersistenceRejected,
            class,
            range: SessionRange::new(first.event.session_time(), last.event.session_time())
                .unwrap_or_else(|_| first.event.affected_range()),
            first_ordinal: first.event.ordinal(),
            last_ordinal: last.event.ordinal(),
            count: events.len() as u64,
            ledger_merged: false,
        };
        let mut state = self.state.lock().expect("event ingress state lock");
        state.gaps.push_entry(entry);
        self.inner
            .dropped_count
            .fetch_add(events.len() as u64, Ordering::AcqRel);
        drop(state);
        self.wake_writer.notify_one();
    }

    fn next_gap_event(&self) -> Option<BrowserEvent> {
        let gap = self
            .state
            .lock()
            .expect("event ingress state lock")
            .gaps
            .pop()?;
        let observed = self.inner.clock.now();
        let ordinal = {
            let mut state = self.state.lock().expect("event ingress state lock");
            allocate(&mut state, self.inner.session_origin, observed)?.0
        };
        let payload = BrowserEventCollectionGap::new(
            gap.reason,
            gap.class,
            gap.range,
            gap.first_ordinal,
            gap.last_ordinal,
            std::num::NonZeroU64::new(gap.count.max(1))?,
            gap.ledger_merged,
        )
        .ok()
        .map(BrowserEventPayload::CollectionGap)?;
        build_event(
            self.inner.as_ref(),
            self.target_id,
            TargetGeneration {
                connection: gap.generation.connection,
                attachment: gap.generation.attachment,
            },
            ordinal,
            gap.range.end(),
            None,
            observed,
            payload,
        )
    }

    fn requeue_gap_event(&self, event: &BrowserEvent) {
        let BrowserEventPayload::CollectionGap(gap) = event.payload() else {
            return;
        };
        let entry = GapEntry {
            generation: TargetGeneration {
                connection: 0,
                attachment: event.attachment_generation(),
            },
            reason: gap.reason(),
            class: gap.affected_class(),
            range: gap.range(),
            first_ordinal: gap.first_ordinal(),
            last_ordinal: gap.last_ordinal(),
            count: gap.count().get(),
            ledger_merged: gap.ledger_merged(),
        };
        self.state
            .lock()
            .expect("event ingress state lock")
            .gaps
            .push_front(entry);
    }

    fn pending_gap_count(&self) -> usize {
        self.state
            .lock()
            .expect("event ingress state lock")
            .gaps
            .len()
    }

    pub(super) fn close(&self) {
        self.state
            .lock()
            .expect("event ingress state lock")
            .accepting = false;
        self.stopping.store(true, Ordering::Release);
        self.stop_writer.notify_waiters();
        self.wake_writer.notify_waiters();
    }
}

fn allocate(
    state: &mut TargetIngressState,
    origin: SessionOrigin,
    observed: ObservedTime,
) -> Option<(BrowserEventOrdinal, SessionTime)> {
    let next = state.last_ordinal.checked_add(1)?;
    let ordinal = BrowserEventOrdinal::new(next).ok()?;
    let normalized = origin.normalize(observed).ok()?;
    let session_time = normalized.max(state.last_session_time);
    state.last_ordinal = next;
    state.last_session_time = session_time;
    Some((ordinal, session_time))
}

#[allow(clippy::too_many_arguments)]
fn build_event(
    inner: &PipelineInner,
    target_id: TargetId,
    generation: TargetGeneration,
    ordinal: BrowserEventOrdinal,
    session_time: SessionTime,
    source_time: Option<BrowserSourceTimestamp>,
    observed: ObservedTime,
    payload: BrowserEventPayload,
) -> Option<BrowserEvent> {
    let event_id = BrowserEventId::from_uuid(*inner.ids.next().as_uuid());
    BrowserEventSeverity::ALL.iter().find_map(|severity| {
        BrowserEvent::new(
            event_id,
            inner.session_id,
            target_id,
            generation.attachment,
            ordinal,
            session_time,
            source_time.clone(),
            observed,
            *severity,
            payload.clone(),
        )
        .ok()
    })
}

fn reserve_pending(inner: &PipelineInner, bytes: usize) -> bool {
    let limit = inner.config.global_pending_bytes.get();
    let mut current = inner.pending_bytes.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes).filter(|next| *next <= limit) else {
            return false;
        };
        match inner.pending_bytes.compare_exchange_weak(
            current,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
}

fn release_pending(inner: &PipelineInner, bytes: usize) {
    inner.pending_bytes.fetch_sub(bytes, Ordering::AcqRel);
}

fn push_gap(
    state: &mut TargetIngressState,
    generation: TargetGeneration,
    reason: BrowserEventGapReason,
    class: Option<BrowserEventClass>,
    at: SessionTime,
    ordinal: BrowserEventOrdinal,
) {
    state.gaps.push_entry(GapEntry {
        generation,
        reason,
        class,
        range: SessionRange::new(at, at).expect("point event gap range is valid"),
        first_ordinal: ordinal,
        last_ordinal: ordinal,
        count: 1,
        ledger_merged: false,
    });
}

impl GapLedger {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn pop(&mut self) -> Option<GapEntry> {
        self.entries.pop_front()
    }

    fn push_front(&mut self, entry: GapEntry) {
        if self.entries.len() >= self.capacity {
            self.merge_oldest();
        }
        self.entries.push_front(entry);
    }

    fn push_entry(&mut self, entry: GapEntry) {
        if let Some(last) = self.entries.back_mut()
            && last.generation == entry.generation
            && last.reason == entry.reason
            && last.class == entry.class
            && last.last_ordinal.get().saturating_add(1) == entry.first_ordinal.get()
        {
            last.range = SessionRange::new(
                last.range.start().min(entry.range.start()),
                last.range.end().max(entry.range.end()),
            )
            .expect("merged event gap range is ordered");
            last.last_ordinal = entry.last_ordinal;
            last.count = last.count.saturating_add(entry.count);
            last.ledger_merged |= entry.ledger_merged;
            return;
        }
        if self.entries.len() >= self.capacity {
            self.merge_oldest();
        }
        self.entries.push_back(entry);
    }

    fn merge_oldest(&mut self) {
        let Some(mut first) = self.entries.pop_front() else {
            return;
        };
        let Some(second) = self.entries.pop_front() else {
            self.entries.push_front(first);
            return;
        };
        first.range = SessionRange::new(
            first.range.start().min(second.range.start()),
            first.range.end().max(second.range.end()),
        )
        .expect("conservative event gap range is ordered");
        first.last_ordinal = first.last_ordinal.max(second.last_ordinal);
        first.count = first.count.saturating_add(second.count);
        first.class = (first.class == second.class)
            .then_some(first.class)
            .flatten();
        first.ledger_merged = true;
        self.entries.push_front(first);
    }
}

async fn writer_loop(
    ingress: Arc<TargetIngress>,
    mut receiver: mpsc::Receiver<QueuedEvent>,
) -> bool {
    let sink = ingress
        .inner
        .sink
        .as_ref()
        .expect("writer exists only with a browser event sink")
        .clone();
    let mut carry = None;
    let mut retry = ingress.inner.config.persistence_retry_initial;
    loop {
        if let Some(gap) = ingress.next_gap_event() {
            let persisted =
                match BrowserEventBatch::new(ingress.inner.session_id, vec![gap.clone()]) {
                    Ok(batch) => sink.append_event_batch(batch).await.is_ok(),
                    Err(_) => false,
                };
            if persisted {
                ingress.inner.persisted_count.fetch_add(1, Ordering::AcqRel);
                retry = ingress.inner.config.persistence_retry_initial;
                continue;
            } else {
                ingress.requeue_gap_event(&gap);
                ingress.inner.mark_failed();
                tokio::time::sleep(retry).await;
                retry = retry
                    .saturating_mul(2)
                    .min(ingress.inner.config.persistence_retry_max);
                if ingress.stopping.load(Ordering::Acquire) {
                    return false;
                }
                continue;
            }
        }

        let first = match carry.take() {
            Some(event) => Some(event),
            None => match receiver.try_recv() {
                Ok(event) => Some(event),
                Err(mpsc::error::TryRecvError::Disconnected) => None,
                Err(mpsc::error::TryRecvError::Empty) => {
                    if ingress.stopping.load(Ordering::Acquire) {
                        return ingress.pending_gap_count() == 0;
                    }
                    tokio::select! {
                        event = receiver.recv() => event,
                        _ = ingress.wake_writer.notified() => continue,
                        _ = ingress.stop_writer.notified() => continue,
                    }
                }
            },
        };
        let Some(first) = first else {
            return ingress.pending_gap_count() == 0;
        };
        let mut events = vec![first];
        let mut bytes = events[0].bytes;
        while events.len() < ingress.inner.config.store_batch_rows.get() {
            match receiver.try_recv() {
                Ok(next)
                    if bytes.saturating_add(next.bytes)
                        <= ingress.inner.config.store_batch_bytes.get() =>
                {
                    bytes = bytes.saturating_add(next.bytes);
                    events.push(next);
                }
                Ok(next) => {
                    carry = Some(next);
                    break;
                }
                Err(_) => break,
            }
        }
        let core_events = events.iter().map(|event| event.event.clone()).collect();
        let persisted = match BrowserEventBatch::new(ingress.inner.session_id, core_events) {
            Ok(batch) => sink.append_event_batch(batch).await.is_ok(),
            Err(_) => false,
        };
        for event in &events {
            release_pending(&ingress.inner, event.bytes);
        }
        if persisted {
            ingress
                .inner
                .persisted_count
                .fetch_add(events.len() as u64, Ordering::AcqRel);
            retry = ingress.inner.config.persistence_retry_initial;
        } else {
            ingress.record_batch_failure(&events);
            ingress.inner.mark_failed();
        }
    }
}

impl PipelineInner {
    fn mark_failed(&self) {
        self.state.lock().expect("event status lock").status = BrowserEventCollectionStatus::Failed;
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::atomic::AtomicU64, time::Duration};

    use krometrail_core::{
        BrowserEventBatch, BrowserEventKind, ErrorCode, IdValue, KrometrailError, NonEmptyText,
        PortFuture, TargetLifecycle, TargetLifecycleEvent,
    };
    use uuid::Uuid;

    use super::*;

    struct TestClock(AtomicU64);

    impl MonotonicClock for TestClock {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(self.0.fetch_add(1, Ordering::AcqRel) + 1)
        }
    }

    struct TestIds(AtomicU64);

    impl IdSource for TestIds {
        fn next(&self) -> IdValue {
            let next = self.0.fetch_add(1, Ordering::AcqRel) + 1;
            IdValue::from_uuid(Uuid::from_u128(u128::from(next)))
        }
    }

    fn target(value: u128) -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(value))
    }

    fn payload() -> BrowserEventPayload {
        BrowserEventPayload::TargetLifecycle(TargetLifecycleEvent::new(TargetLifecycle::Attached))
    }

    #[test]
    fn bounded_gap_ledger_coalesces_consecutive_and_pressure_ranges() {
        let generation = TargetGeneration {
            connection: 1,
            attachment: 1,
        };
        let mut ledger = GapLedger::new(2);
        for ordinal in 1..=4 {
            ledger.push_entry(GapEntry {
                generation,
                reason: if ordinal % 2 == 0 {
                    BrowserEventGapReason::QueueSaturated
                } else {
                    BrowserEventGapReason::InvalidPayload
                },
                class: Some(BrowserEventClass::Console),
                range: SessionRange::new(
                    SessionTime::from_nanos(ordinal),
                    SessionTime::from_nanos(ordinal),
                )
                .unwrap(),
                first_ordinal: BrowserEventOrdinal::new(ordinal).unwrap(),
                last_ordinal: BrowserEventOrdinal::new(ordinal).unwrap(),
                count: 1,
                ledger_merged: false,
            });
        }
        assert!(ledger.len() <= 2);
        assert!(ledger.entries.iter().map(|gap| gap.count).sum::<u64>() == 4);
        assert!(ledger.entries.iter().any(|gap| gap.ledger_merged));
    }

    #[test]
    fn stale_generation_cannot_allocate_an_ordinal() {
        let pipeline = EventPipeline::new(
            SessionId::from_uuid(Uuid::from_u128(10)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
            Arc::new(TestClock(AtomicU64::new(0))),
            Arc::new(TestIds(AtomicU64::new(0))),
            None,
            BrowserEventConfig::disabled(),
        )
        .unwrap();
        let target = target(11);
        let old = TargetGeneration {
            connection: 1,
            attachment: 3,
        };
        let ingress = pipeline.begin_target(target, old).unwrap();
        assert_eq!(
            ingress.submit_payload(old, ObservedTime::from_nanos(1), None, payload()),
            SubmitOutcome::Disabled
        );
        ingress.suspend_generation(old, BrowserEventGapReason::ReconnectBoundary);
        let before_late = ingress
            .state
            .lock()
            .expect("event ingress state lock")
            .last_ordinal;

        let current = TargetGeneration {
            connection: 2,
            attachment: 1,
        };
        assert!(pipeline.begin_target(target, current).is_ok());
        assert_eq!(
            ingress.submit_payload(old, ObservedTime::from_nanos(2), None, payload()),
            SubmitOutcome::StaleGeneration
        );
        assert_eq!(
            ingress
                .state
                .lock()
                .expect("event ingress state lock")
                .last_ordinal,
            before_late
        );
        assert_eq!(
            ingress.submit_payload(current, ObservedTime::from_nanos(3), None, payload()),
            SubmitOutcome::Disabled
        );
        assert_eq!(
            ingress
                .state
                .lock()
                .expect("event ingress state lock")
                .last_ordinal,
            before_late + 1
        );
    }

    struct SelectiveGateSink {
        blocked_target: TargetId,
        block_once: AtomicBool,
        started: tokio::sync::Notify,
        release: tokio::sync::Semaphore,
        persisted: Mutex<Vec<TargetId>>,
    }

    impl SelectiveGateSink {
        fn new(blocked_target: TargetId) -> Arc<Self> {
            Arc::new(Self {
                blocked_target,
                block_once: AtomicBool::new(true),
                started: tokio::sync::Notify::new(),
                release: tokio::sync::Semaphore::new(0),
                persisted: Mutex::new(Vec::new()),
            })
        }

        async fn wait_until_blocked(&self) {
            if !self.block_once.load(Ordering::Acquire) {
                return;
            }
            self.started.notified().await;
        }

        async fn wait_until_persisted(&self, target_id: TargetId) {
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    let notified = self.started.notified();
                    if self
                        .persisted
                        .lock()
                        .expect("persisted target lock")
                        .contains(&target_id)
                    {
                        return;
                    }
                    notified.await;
                }
            })
            .await
            .expect("other target was starved");
        }
    }

    impl BrowserEventSink for SelectiveGateSink {
        fn append_event_batch(
            &self,
            batch: BrowserEventBatch,
        ) -> PortFuture<'_, krometrail_core::Result<()>> {
            Box::pin(async move {
                let target_id = batch.events()[0].target_id();
                if target_id == self.blocked_target && self.block_once.swap(false, Ordering::AcqRel)
                {
                    self.started.notify_waiters();
                    self.release
                        .acquire()
                        .await
                        .expect("gate remains open")
                        .forget();
                }
                self.persisted
                    .lock()
                    .expect("persisted target lock")
                    .push(target_id);
                self.started.notify_waiters();
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn saturated_target_does_not_starve_another_target_and_shutdown_flushes() {
        let blocked_target = target(21);
        let other_target = target(22);
        let sink = SelectiveGateSink::new(blocked_target);
        let config = BrowserEventConfig {
            per_target_queue_capacity: std::num::NonZeroUsize::new(1).unwrap(),
            ..BrowserEventConfig::default()
        };
        let pipeline = EventPipeline::new(
            SessionId::from_uuid(Uuid::from_u128(20)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
            Arc::new(TestClock(AtomicU64::new(0))),
            Arc::new(TestIds(AtomicU64::new(0))),
            Some(sink.clone() as Arc<dyn BrowserEventSink>),
            config,
        )
        .unwrap();
        let generation = TargetGeneration {
            connection: 1,
            attachment: 1,
        };
        let blocked = pipeline.begin_target(blocked_target, generation).unwrap();
        let other = pipeline.begin_target(other_target, generation).unwrap();
        assert_eq!(
            blocked.submit_payload(generation, ObservedTime::from_nanos(1), None, payload()),
            SubmitOutcome::Accepted
        );
        sink.wait_until_blocked().await;
        assert_eq!(
            blocked.submit_payload(generation, ObservedTime::from_nanos(2), None, payload()),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            blocked.submit_payload(generation, ObservedTime::from_nanos(3), None, payload()),
            SubmitOutcome::Dropped
        );
        assert_eq!(
            other.submit_payload(generation, ObservedTime::from_nanos(4), None, payload()),
            SubmitOutcome::Accepted
        );
        sink.wait_until_persisted(other_target).await;

        sink.release.add_permits(1);
        assert!(
            pipeline
                .shutdown(tokio::time::Instant::now() + Duration::from_secs(1))
                .await
        );
        assert_eq!(pipeline.status().pending_bytes, 0);
    }

    struct FailOnceSink {
        fail_next: AtomicBool,
        events: Mutex<Vec<BrowserEvent>>,
        changed: tokio::sync::Notify,
    }

    impl BrowserEventSink for FailOnceSink {
        fn append_event_batch(
            &self,
            batch: BrowserEventBatch,
        ) -> PortFuture<'_, krometrail_core::Result<()>> {
            Box::pin(async move {
                if self.fail_next.swap(false, Ordering::AcqRel) {
                    return Err(KrometrailError::new(
                        ErrorCode::PersistenceFailed,
                        NonEmptyText::new("deliberate event sink rejection").unwrap(),
                    ));
                }
                self.events
                    .lock()
                    .expect("failed-sink event lock")
                    .extend(batch.events().iter().cloned());
                self.changed.notify_waiters();
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn rejected_persistence_becomes_a_bounded_collection_gap() {
        let sink = Arc::new(FailOnceSink {
            fail_next: AtomicBool::new(true),
            events: Mutex::new(Vec::new()),
            changed: tokio::sync::Notify::new(),
        });
        let pipeline = EventPipeline::new(
            SessionId::from_uuid(Uuid::from_u128(25)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
            Arc::new(TestClock(AtomicU64::new(0))),
            Arc::new(TestIds(AtomicU64::new(0))),
            Some(sink.clone() as Arc<dyn BrowserEventSink>),
            BrowserEventConfig::default(),
        )
        .unwrap();
        let generation = TargetGeneration {
            connection: 1,
            attachment: 1,
        };
        let ingress = pipeline.begin_target(target(26), generation).unwrap();
        assert_eq!(
            ingress.submit_payload(generation, ObservedTime::from_nanos(1), None, payload()),
            SubmitOutcome::Accepted
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let notified = sink.changed.notified();
                if sink
                    .events
                    .lock()
                    .expect("failed-sink event lock")
                    .iter()
                    .any(|event| event.kind() == BrowserEventKind::CollectionGap)
                {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("collection gap was not persisted");
        let status = pipeline.status();
        assert_eq!(status.state, BrowserEventCollectionStatus::Failed);
        assert_eq!(status.dropped_count, 1);
        assert!(
            pipeline
                .shutdown(tokio::time::Instant::now() + Duration::from_secs(1))
                .await
        );
    }

    #[tokio::test]
    async fn shutdown_aborts_a_writer_that_exceeds_the_absolute_deadline() {
        let blocked_target = target(31);
        let sink = SelectiveGateSink::new(blocked_target);
        let pipeline = EventPipeline::new(
            SessionId::from_uuid(Uuid::from_u128(30)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
            Arc::new(TestClock(AtomicU64::new(0))),
            Arc::new(TestIds(AtomicU64::new(0))),
            Some(sink.clone() as Arc<dyn BrowserEventSink>),
            BrowserEventConfig::default(),
        )
        .unwrap();
        let generation = TargetGeneration {
            connection: 1,
            attachment: 1,
        };
        let ingress = pipeline.begin_target(blocked_target, generation).unwrap();
        assert_eq!(
            ingress.submit_payload(generation, ObservedTime::from_nanos(1), None, payload()),
            SubmitOutcome::Accepted
        );
        sink.wait_until_blocked().await;
        assert!(
            !pipeline
                .shutdown(tokio::time::Instant::now() + Duration::from_millis(20))
                .await
        );
        assert_eq!(
            pipeline.status().state,
            BrowserEventCollectionStatus::Failed
        );
    }
}
