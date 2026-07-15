use std::{
    collections::HashMap,
    future::pending,
    sync::{
        Arc, Mutex as StdMutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

use krometrail_core::{ArtifactCacheKey, CancellationSignal, KrometrailError, StoredArtifact};
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore};

use super::{
    cache::{DecodedFrameKey, NormalizedFrameKey},
    epoch::WorkCancellation,
    scheduler::{cancelled_error, deadline_error},
};
use temporal_vision::{SharedFrame, SharedNormalizedFrame};

#[derive(Clone)]
pub(crate) struct FlightValue {
    pub artifact: StoredArtifact,
    pub generated: bool,
}

pub(crate) type FlightArtifacts =
    HashMap<ArtifactCacheKey, std::result::Result<FlightValue, KrometrailError>>;
pub(crate) type FlightResult = std::result::Result<FlightArtifacts, KrometrailError>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FlightKey(Vec<ArtifactCacheKey>);

pub(crate) struct SingleFlight {
    flights: StdMutex<HashMap<FlightKey, Weak<Flight>>>,
}

pub(crate) struct Flight {
    result: Mutex<Option<Arc<FlightResult>>>,
    notify: Notify,
    waiters: StdMutex<usize>,
    cancellation: WorkCancellation,
}

pub(crate) struct FlightWaiter {
    flight: Arc<Flight>,
    pub is_leader: bool,
}

impl SingleFlight {
    pub(crate) fn new() -> Self {
        Self {
            flights: StdMutex::new(HashMap::new()),
        }
    }

    pub(crate) fn join(&self, keys: Vec<ArtifactCacheKey>) -> FlightWaiter {
        let key = FlightKey(keys);
        let mut flights = self
            .flights
            .lock()
            .expect("artifact flight registry poisoned");
        flights.retain(|_, flight| flight.strong_count() > 0);
        let (flight, is_leader) = match flights.get(&key).and_then(Weak::upgrade) {
            Some(flight) => (flight, false),
            None => {
                let flight = Arc::new(Flight {
                    result: Mutex::new(None),
                    notify: Notify::new(),
                    waiters: StdMutex::new(0),
                    cancellation: WorkCancellation::default(),
                });
                flights.insert(key, Arc::downgrade(&flight));
                (flight, true)
            }
        };
        *flight
            .waiters
            .lock()
            .expect("artifact flight waiter count poisoned") += 1;
        FlightWaiter { flight, is_leader }
    }
}

impl Flight {
    pub(crate) fn cancellation(&self) -> WorkCancellation {
        self.cancellation.clone()
    }

    pub(crate) async fn complete(&self, result: FlightResult) {
        *self.result.lock().await = Some(Arc::new(result));
        self.notify.notify_waiters();
    }

    async fn result(&self) -> Option<Arc<FlightResult>> {
        self.result.lock().await.clone()
    }
}

impl FlightWaiter {
    pub(crate) fn flight(&self) -> Arc<Flight> {
        Arc::clone(&self.flight)
    }

    pub(crate) async fn wait(
        self,
        deadline: Instant,
        cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> std::result::Result<FlightArtifacts, KrometrailError> {
        loop {
            let notified = self.flight.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = self.flight.result().await {
                return result.as_ref().clone();
            }
            tokio::select! {
                () = notified.as_mut() => {}
                () = external_cancelled(cancellation.as_ref()) => return Err(cancelled_error()),
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                    return Err(deadline_error());
                }
            }
        }
    }
}

impl Drop for FlightWaiter {
    fn drop(&mut self) {
        let mut waiters = self
            .flight
            .waiters
            .lock()
            .expect("artifact flight waiter count poisoned");
        *waiters = waiters.saturating_sub(1);
        if *waiters == 0 {
            self.flight.cancellation.cancel();
        }
    }
}

async fn external_cancelled(cancellation: Option<&Arc<dyn CancellationSignal>>) {
    match cancellation {
        Some(signal) => signal.cancelled().await,
        None => pending().await,
    }
}

#[allow(dead_code)]
#[derive(Debug)]
enum WorkFlightState<T> {
    InFlight,
    Ready { value: Arc<T>, retained: bool },
    Failed(KrometrailError),
}

#[allow(dead_code)]
struct WorkByteBudget {
    maximum: usize,
    used: AtomicUsize,
    memory: Option<Arc<Semaphore>>,
}

struct WorkReservation {
    bytes: usize,
    _memory: Option<OwnedSemaphorePermit>,
}

#[allow(dead_code)]
impl WorkByteBudget {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            used: AtomicUsize::new(0),
            memory: None,
        }
    }

    fn with_memory(maximum: usize, memory: Arc<Semaphore>) -> Self {
        Self {
            maximum,
            used: AtomicUsize::new(0),
            memory: Some(memory),
        }
    }

    fn try_reserve(&self, bytes: usize) -> Option<WorkReservation> {
        if bytes == 0 || bytes > self.maximum {
            return None;
        }
        let mut current = self.used.load(Ordering::Acquire);
        loop {
            let next = current.checked_add(bytes)?;
            if next > self.maximum {
                return None;
            }
            if self
                .used
                .compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let permits = match u32::try_from(bytes) {
                    Ok(permits) => permits,
                    Err(_) => {
                        self.release(bytes);
                        return None;
                    }
                };
                let memory = self
                    .memory
                    .as_ref()
                    .map(|memory| memory.clone().try_acquire_many_owned(permits));
                match memory {
                    Some(Ok(permit)) => {
                        return Some(WorkReservation {
                            bytes,
                            _memory: Some(permit),
                        });
                    }
                    Some(Err(_)) => {
                        self.release(bytes);
                        return None;
                    }
                    None => {
                        return Some(WorkReservation {
                            bytes,
                            _memory: None,
                        });
                    }
                }
            }
            current = self.used.load(Ordering::Acquire);
        }
    }

    fn release(&self, bytes: usize) {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(
                    current
                        .checked_sub(bytes)
                        .expect("work-byte accounting underflow"),
                )
            })
            .expect("work-byte accounting update must succeed");
    }
}

#[allow(dead_code)]
pub(crate) struct WorkFlight<T> {
    state: Mutex<WorkFlightState<T>>,
    notify: Notify,
    waiters: StdMutex<usize>,
    cancellation: WorkCancellation,
    finished: AtomicBool,
    budget: Arc<WorkByteBudget>,
    reservation: StdMutex<Option<WorkReservation>>,
}

#[allow(dead_code)]
impl<T> WorkFlight<T> {
    fn new(budget: Arc<WorkByteBudget>) -> Self {
        Self {
            state: Mutex::new(WorkFlightState::InFlight),
            notify: Notify::new(),
            waiters: StdMutex::new(0),
            cancellation: WorkCancellation::default(),
            finished: AtomicBool::new(false),
            budget,
            reservation: StdMutex::new(None),
        }
    }

    pub(crate) fn cancellation(&self) -> WorkCancellation {
        self.cancellation.clone()
    }

    pub(crate) fn waiter_count(&self) -> usize {
        *self
            .waiters
            .lock()
            .expect("work-flight waiter count poisoned")
    }

    fn add_waiter(self: &Arc<Self>, is_leader: bool) -> WorkWaiter<T> {
        *self
            .waiters
            .lock()
            .expect("work-flight waiter count poisoned") += 1;
        WorkWaiter {
            flight: Arc::clone(self),
            is_leader,
        }
    }

    fn reserve(&self, bytes: usize) -> bool {
        let Some(reservation) = self.budget.try_reserve(bytes) else {
            return false;
        };
        let mut slot = self
            .reservation
            .lock()
            .expect("work-flight reservation poisoned");
        if slot.is_some() {
            self.budget.release(reservation.bytes);
            return false;
        }
        *slot = Some(reservation);
        true
    }

    fn release_reservation(&self) {
        let reservation = self
            .reservation
            .lock()
            .expect("work-flight reservation poisoned")
            .take();
        if let Some(reservation) = reservation {
            self.budget.release(reservation.bytes);
        }
    }

    async fn publish_ready(&self, value: T, retained: bool) -> bool {
        let mut state = self.state.lock().await;
        if self.cancellation.is_cancelled() || !matches!(*state, WorkFlightState::InFlight) {
            return false;
        }
        *state = WorkFlightState::Ready {
            value: Arc::new(value),
            retained,
        };
        self.finished.store(true, Ordering::Release);
        drop(state);
        self.notify.notify_waiters();
        true
    }

    async fn publish_failed(&self, error: KrometrailError) -> bool {
        let mut state = self.state.lock().await;
        if !matches!(*state, WorkFlightState::InFlight) {
            return false;
        }
        *state = WorkFlightState::Failed(error);
        self.finished.store(true, Ordering::Release);
        drop(state);
        self.notify.notify_waiters();
        true
    }

    async fn wait(
        &self,
        deadline: Instant,
        cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> std::result::Result<WorkValue<T>, KrometrailError> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = self.state.lock().await;
                match &*state {
                    WorkFlightState::Ready { value, retained } => {
                        return Ok(WorkValue {
                            value: Arc::clone(value),
                            retained: *retained,
                        });
                    }
                    WorkFlightState::Failed(error) => return Err(error.clone()),
                    WorkFlightState::InFlight => {}
                }
            }
            tokio::select! {
                () = notified.as_mut() => {}
                () = external_cancelled(cancellation.as_ref()) => return Err(cancelled_error()),
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                    return Err(deadline_error());
                }
            }
        }
    }
}

impl<T> Drop for WorkFlight<T> {
    fn drop(&mut self) {
        let reservation = self
            .reservation
            .get_mut()
            .expect("work-flight reservation poisoned")
            .take();
        if let Some(reservation) = reservation {
            self.budget.release(reservation.bytes);
        }
    }
}

#[allow(dead_code)]
pub(crate) type DecodedWorkFlight = WorkFlight<SharedFrame<krometrail_core::FrameId>>;
#[allow(dead_code)]
pub(crate) type NormalizedWorkFlight = WorkFlight<SharedNormalizedFrame<krometrail_core::FrameId>>;

pub(crate) struct WorkValue<T> {
    value: Arc<T>,
    retained: bool,
}

impl<T> WorkValue<T> {
    pub(crate) fn retained(&self) -> bool {
        self.retained
    }

    pub(crate) fn into_arc(self) -> Arc<T> {
        self.value
    }
}

impl<T> std::ops::Deref for WorkValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[allow(dead_code)]
pub(crate) struct WorkWaiter<T> {
    flight: Arc<WorkFlight<T>>,
    pub(crate) is_leader: bool,
}

#[allow(dead_code)]
impl<T> WorkWaiter<T> {
    pub(crate) fn flight(&self) -> Arc<WorkFlight<T>> {
        Arc::clone(&self.flight)
    }

    pub(crate) async fn wait(
        self,
        deadline: Instant,
        cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> std::result::Result<WorkValue<T>, KrometrailError> {
        self.flight.wait(deadline, cancellation).await
    }
}

impl<T> Drop for WorkWaiter<T> {
    fn drop(&mut self) {
        let mut waiters = self
            .flight
            .waiters
            .lock()
            .expect("work-flight waiter count poisoned");
        *waiters = waiters.saturating_sub(1);
        if *waiters == 0 && !self.flight.finished.load(Ordering::Acquire) {
            self.flight.cancellation.cancel();
        }
    }
}

#[allow(dead_code)]
struct WorkBatchState {
    decoded: StdMutex<HashMap<DecodedFrameKey, Arc<DecodedWorkFlight>>>,
    normalized: StdMutex<HashMap<NormalizedFrameKey, Arc<NormalizedWorkFlight>>>,
}

/// A lease on all intermediate frame work used by one generation request. The registry stores
/// only weak references; dropping the last lease therefore ends the reuse window.
#[allow(dead_code)]
pub(crate) struct WorkBatchLease {
    state: Arc<WorkBatchState>,
}

impl Clone for WorkBatchLease {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

#[allow(dead_code)]
pub(crate) struct WorkBatchRegistry {
    decoded: StdMutex<HashMap<DecodedFrameKey, Weak<DecodedWorkFlight>>>,
    normalized: StdMutex<HashMap<NormalizedFrameKey, Weak<NormalizedWorkFlight>>>,
    budget: Arc<WorkByteBudget>,
}

#[allow(dead_code)]
impl WorkBatchRegistry {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            decoded: StdMutex::new(HashMap::new()),
            normalized: StdMutex::new(HashMap::new()),
            budget: Arc::new(WorkByteBudget::new(max_bytes)),
        }
    }

    pub(crate) fn new_with_memory(max_bytes: usize, memory: Arc<Semaphore>) -> Self {
        Self {
            decoded: StdMutex::new(HashMap::new()),
            normalized: StdMutex::new(HashMap::new()),
            budget: Arc::new(WorkByteBudget::with_memory(max_bytes, memory)),
        }
    }

    pub(crate) fn begin_batch(&self) -> WorkBatchLease {
        WorkBatchLease {
            state: Arc::new(WorkBatchState {
                decoded: StdMutex::new(HashMap::new()),
                normalized: StdMutex::new(HashMap::new()),
            }),
        }
    }

    pub(crate) fn bytes_used(&self) -> usize {
        self.budget.used.load(Ordering::Acquire)
    }

    pub(crate) fn entry_count(&self) -> usize {
        let mut decoded = self.decoded.lock().expect("decoded registry poisoned");
        decoded.retain(|_, entry| entry.strong_count() > 0);
        let mut normalized = self
            .normalized
            .lock()
            .expect("normalized registry poisoned");
        normalized.retain(|_, entry| entry.strong_count() > 0);
        decoded.len() + normalized.len()
    }

    pub(crate) fn join_decoded(
        &self,
        batch: &WorkBatchLease,
        key: DecodedFrameKey,
    ) -> WorkWaiter<SharedFrame<krometrail_core::FrameId>> {
        let (flight, is_leader) = {
            let mut entries = self.decoded.lock().expect("decoded registry poisoned");
            entries.retain(|_, entry| entry.strong_count() > 0);
            match entries
                .get(&key)
                .and_then(Weak::upgrade)
                .filter(|flight| !flight.cancellation().is_cancelled())
            {
                Some(flight) => (flight, false),
                None => {
                    let flight = Arc::new(WorkFlight::new(Arc::clone(&self.budget)));
                    entries.insert(key.clone(), Arc::downgrade(&flight));
                    (flight, true)
                }
            }
        };
        batch
            .state
            .decoded
            .lock()
            .expect("work-batch decoded state poisoned")
            .insert(key, Arc::clone(&flight));
        flight.add_waiter(is_leader)
    }

    pub(crate) fn join_normalized(
        &self,
        batch: &WorkBatchLease,
        key: NormalizedFrameKey,
    ) -> WorkWaiter<SharedNormalizedFrame<krometrail_core::FrameId>> {
        let (flight, is_leader) = {
            let mut entries = self
                .normalized
                .lock()
                .expect("normalized registry poisoned");
            entries.retain(|_, entry| entry.strong_count() > 0);
            match entries
                .get(&key)
                .and_then(Weak::upgrade)
                .filter(|flight| !flight.cancellation().is_cancelled())
            {
                Some(flight) => (flight, false),
                None => {
                    let flight = Arc::new(WorkFlight::new(Arc::clone(&self.budget)));
                    entries.insert(key.clone(), Arc::downgrade(&flight));
                    (flight, true)
                }
            }
        };
        batch
            .state
            .normalized
            .lock()
            .expect("work-batch normalized state poisoned")
            .insert(key, Arc::clone(&flight));
        flight.add_waiter(is_leader)
    }

    pub(crate) async fn complete_decoded(
        &self,
        key: &DecodedFrameKey,
        flight: &Arc<DecodedWorkFlight>,
        value: SharedFrame<krometrail_core::FrameId>,
    ) -> bool {
        let retained = flight.reserve(value.pixels().len());
        let published = flight.publish_ready(value, retained).await;
        if !published {
            flight.release_reservation();
        }
        if !retained || !published {
            self.remove_decoded(key, flight);
        }
        retained && published
    }

    pub(crate) async fn complete_normalized(
        &self,
        key: &NormalizedFrameKey,
        flight: &Arc<NormalizedWorkFlight>,
        value: SharedNormalizedFrame<krometrail_core::FrameId>,
    ) -> bool {
        let bytes = value
            .linear_rgb16()
            .len()
            .saturating_mul(std::mem::size_of::<u16>());
        let retained = flight.reserve(bytes);
        let published = flight.publish_ready(value, retained).await;
        if !published {
            flight.release_reservation();
        }
        if !retained || !published {
            self.remove_normalized(key, flight);
        }
        retained && published
    }

    pub(crate) async fn fail_decoded(
        &self,
        key: &DecodedFrameKey,
        flight: &Arc<DecodedWorkFlight>,
        error: KrometrailError,
    ) {
        self.remove_decoded(key, flight);
        flight.publish_failed(error).await;
    }

    pub(crate) async fn fail_normalized(
        &self,
        key: &NormalizedFrameKey,
        flight: &Arc<NormalizedWorkFlight>,
        error: KrometrailError,
    ) {
        self.remove_normalized(key, flight);
        flight.publish_failed(error).await;
    }

    fn remove_decoded(&self, key: &DecodedFrameKey, flight: &Arc<DecodedWorkFlight>) {
        let mut entries = self.decoded.lock().expect("decoded registry poisoned");
        if entries
            .get(key)
            .and_then(Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, flight))
        {
            entries.remove(key);
        }
    }

    fn remove_normalized(&self, key: &NormalizedFrameKey, flight: &Arc<NormalizedWorkFlight>) {
        let mut entries = self
            .normalized
            .lock()
            .expect("normalized registry poisoned");
        if entries
            .get(key)
            .and_then(Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, flight))
        {
            entries.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn completion_after_notification_registration_is_not_lost() {
        let flights = SingleFlight::new();
        let waiter = flights.join(vec![ArtifactCacheKey::from_bytes([1; 32])]);
        let flight = waiter.flight();
        let notified = flight.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        flight.complete(Ok(HashMap::new())).await;

        tokio::time::timeout(std::time::Duration::from_millis(100), notified)
            .await
            .expect("registered completion notification");
        assert!(flight.result().await.unwrap().as_ref().is_ok());
    }

    fn decoded_key(index: u128) -> DecodedFrameKey {
        DecodedFrameKey {
            session_id: krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(1)),
            target_id: krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(2)),
            frame_id: krometrail_core::FrameId::from_uuid(uuid::Uuid::from_u128(index + 10)),
            capture_ordinal: (index as u64).saturating_add(1),
            session_time_nanos: index as u64,
            source_format: krometrail_core::ImageFormat::Png,
            image_dimensions: krometrail_core::PixelDimensions::new(1, 1).unwrap(),
            viewport_dimensions: krometrail_core::PixelDimensions::new(1, 1).unwrap(),
            device_scale_bits: 1.0_f64.to_bits(),
            encoded_sha256: [index as u8; 32],
            visual_epoch_hash: [7; 32],
            decoder_profile: Arc::from("decoder"),
            decoder_algorithm_version: Arc::from("decoder-v1"),
        }
    }

    fn normalized_key(index: u128) -> NormalizedFrameKey {
        NormalizedFrameKey::new(
            decoded_key(index),
            [7; 32],
            temporal_vision::PixelRect::new(0, 0, 1, 1).unwrap(),
            temporal_vision::IntegerScale::IDENTITY,
            temporal_vision::Rgb8::new(0, 0, 0),
            [8; 32],
            "recipe-v1",
            "lut-v1",
            "normalizer-v1",
        )
    }

    fn decoded_value(index: u128) -> SharedFrame<krometrail_core::FrameId> {
        temporal_vision::Frame::new(
            krometrail_core::FrameId::from_uuid(uuid::Uuid::from_u128(index + 10)),
            temporal_vision::Timestamp::from_nanos(index as u64),
            temporal_vision::PixelDimensions::new(1, 1).unwrap(),
            temporal_vision::PixelFormat::Rgba8SrgbStraight,
            Arc::from([index as u8, 2, 3, 255]),
        )
        .unwrap()
    }

    fn normalized_value(index: u128) -> SharedNormalizedFrame<krometrail_core::FrameId> {
        temporal_vision::NormalizedFrame::new(
            krometrail_core::FrameId::from_uuid(uuid::Uuid::from_u128(index + 10)),
            temporal_vision::Timestamp::from_nanos(index as u64),
            temporal_vision::PixelDimensions::new(1, 1).unwrap(),
            Arc::from([index as u16, 2, 3]),
        )
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn work_flight_suppresses_duplicate_leader_and_releases_after_batch() {
        let registry = WorkBatchRegistry::new(64);
        let batch = registry.begin_batch();
        let first = registry.join_decoded(&batch, decoded_key(1));
        let flight = first.flight();
        let second = registry.join_decoded(&batch, decoded_key(1));
        assert!(first.is_leader);
        assert!(!second.is_leader);
        registry
            .complete_decoded(&decoded_key(1), &flight, decoded_value(1))
            .await;
        let first_pixels = first
            .wait(Instant::now() + std::time::Duration::from_secs(1), None)
            .await
            .unwrap();
        let second_pixels = second
            .wait(Instant::now() + std::time::Duration::from_secs(1), None)
            .await
            .unwrap();
        assert_eq!(first_pixels.pixels(), second_pixels.pixels());
        assert_eq!(
            first_pixels.pixels().as_ptr(),
            second_pixels.pixels().as_ptr(),
            "waiters must observe one immutable pixel allocation"
        );
        assert_eq!(registry.bytes_used(), 4);
        assert_eq!(registry.entry_count(), 1);
        drop(flight);
        drop(batch);
        assert_eq!(registry.bytes_used(), 0);
        assert_eq!(registry.entry_count(), 0);

        let later_batch = registry.begin_batch();
        assert!(
            registry
                .join_decoded(&later_batch, decoded_key(1))
                .is_leader
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn normalized_work_flight_accounts_bytes_and_failure_is_not_cached() {
        let registry = WorkBatchRegistry::new(6);
        let batch = registry.begin_batch();
        let first = registry.join_normalized(&batch, normalized_key(1));
        let flight = first.flight();
        let second = registry.join_normalized(&batch, normalized_key(1));
        assert!(first.is_leader);
        assert!(!second.is_leader);
        registry
            .complete_normalized(&normalized_key(1), &flight, normalized_value(1))
            .await;
        let first_pixels = first
            .wait(Instant::now() + std::time::Duration::from_secs(1), None)
            .await
            .unwrap();
        let second_pixels = second
            .wait(Instant::now() + std::time::Duration::from_secs(1), None)
            .await
            .unwrap();
        assert_eq!(first_pixels.linear_rgb16(), second_pixels.linear_rgb16());
        assert_eq!(registry.bytes_used(), 6);
        drop(flight);
        drop(batch);
        assert_eq!(registry.bytes_used(), 0);

        let failed_batch = registry.begin_batch();
        let failed = registry.join_decoded(&failed_batch, decoded_key(2));
        let failed_flight = failed.flight();
        registry
            .fail_decoded(
                &decoded_key(2),
                &failed_flight,
                KrometrailError::new(
                    krometrail_core::ErrorCode::ArtifactGenerationFailed,
                    krometrail_core::NonEmptyText::new("decode failed").unwrap(),
                ),
            )
            .await;
        assert!(
            failed
                .wait(Instant::now() + std::time::Duration::from_secs(1), None)
                .await
                .is_err()
        );
        assert_eq!(registry.bytes_used(), 0);
        assert!(
            registry
                .join_decoded(&failed_batch, decoded_key(2))
                .is_leader
        );
    }

    #[test]
    fn a_nonadmitted_entry_is_removed_for_later_work() {
        let registry = WorkBatchRegistry::new(3);
        let batch = registry.begin_batch();
        let waiter = registry.join_decoded(&batch, decoded_key(3));
        let flight = waiter.flight();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(!runtime.block_on(registry.complete_decoded(
            &decoded_key(3),
            &flight,
            decoded_value(3),
        )));
        drop(waiter);
        assert_eq!(registry.bytes_used(), 0);
        assert!(registry.join_decoded(&batch, decoded_key(3)).is_leader);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn overlapping_batches_run_each_of_119_shared_frames_once_for_both_work_kinds() {
        let registry = WorkBatchRegistry::new(121 * (4 + 6));
        let first_batch = registry.begin_batch();
        let second_batch = registry.begin_batch();
        let mut decoded_waiters = Vec::new();
        let mut decoded_leaders = Vec::new();
        for index in 0..120_u128 {
            let waiter = registry.join_decoded(&first_batch, decoded_key(index));
            if waiter.is_leader {
                decoded_leaders.push((decoded_key(index), waiter.flight()));
            }
            decoded_waiters.push(waiter);
        }
        for index in 1..=120_u128 {
            let waiter = registry.join_decoded(&second_batch, decoded_key(index));
            if waiter.is_leader {
                decoded_leaders.push((decoded_key(index), waiter.flight()));
            }
            decoded_waiters.push(waiter);
        }
        assert_eq!(
            decoded_leaders.len(),
            121,
            "119 overlap keys share their leader"
        );
        for (key, flight) in &decoded_leaders {
            registry
                .complete_decoded(
                    key,
                    flight,
                    decoded_value(u128::from(key.capture_ordinal - 1)),
                )
                .await;
        }
        for waiter in decoded_waiters {
            waiter
                .wait(Instant::now() + std::time::Duration::from_secs(1), None)
                .await
                .unwrap();
        }
        drop(decoded_leaders);
        assert_eq!(registry.bytes_used(), 121 * 4);

        let mut normalized_waiters = Vec::new();
        let mut normalized_leaders = Vec::new();
        for index in 0..120_u128 {
            let waiter = registry.join_normalized(&first_batch, normalized_key(index));
            if waiter.is_leader {
                normalized_leaders.push((normalized_key(index), waiter.flight()));
            }
            normalized_waiters.push(waiter);
        }
        for index in 1..=120_u128 {
            let waiter = registry.join_normalized(&second_batch, normalized_key(index));
            if waiter.is_leader {
                normalized_leaders.push((normalized_key(index), waiter.flight()));
            }
            normalized_waiters.push(waiter);
        }
        assert_eq!(
            normalized_leaders.len(),
            121,
            "119 overlap keys share their leader"
        );
        for (key, flight) in &normalized_leaders {
            registry
                .complete_normalized(
                    key,
                    flight,
                    normalized_value(u128::from(key.decoded.capture_ordinal - 1)),
                )
                .await;
        }
        for waiter in normalized_waiters {
            waiter
                .wait(Instant::now() + std::time::Duration::from_secs(1), None)
                .await
                .unwrap();
        }
        drop(normalized_leaders);
        assert_eq!(registry.bytes_used(), 121 * (4 + 6));
        drop(first_batch);
        assert_eq!(registry.bytes_used(), 120 * (4 + 6));
        drop(second_batch);
        assert_eq!(registry.bytes_used(), 0);
        assert_eq!(registry.entry_count(), 0);
    }

    #[test]
    fn work_waiter_cancellation_is_independent_until_the_last_waiter() {
        let registry = WorkBatchRegistry::new(64);
        let batch = registry.begin_batch();
        let first = registry.join_decoded(&batch, decoded_key(4));
        let flight = first.flight();
        let second = registry.join_decoded(&batch, decoded_key(4));
        drop(first);
        assert!(!flight.cancellation().is_cancelled());
        drop(second);
        assert!(flight.cancellation().is_cancelled());
        assert!(registry.join_decoded(&batch, decoded_key(4)).is_leader);
    }

    #[test]
    fn only_the_last_waiter_cancels_shared_work() {
        let flights = SingleFlight::new();
        let first = flights.join(vec![ArtifactCacheKey::from_bytes([1; 32])]);
        let flight = first.flight();
        let second = flights.join(vec![ArtifactCacheKey::from_bytes([1; 32])]);
        assert!(first.is_leader);
        assert!(!second.is_leader);
        drop(first);
        assert!(!flight.cancellation().is_cancelled());
        drop(second);
        assert!(flight.cancellation().is_cancelled());
    }
}
