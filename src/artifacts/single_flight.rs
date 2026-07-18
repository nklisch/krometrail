use std::{
    collections::HashMap,
    future::pending,
    sync::{Arc, Mutex as StdMutex, Weak},
    time::Instant,
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactManifest, CancellationSignal, KrometrailError, NonEmptyText,
    StoredArtifact,
};
use tokio::sync::{Mutex, Notify};

use super::{
    epoch::WorkCancellation,
    scheduler::{cancelled_error, deadline_error},
};

#[derive(Clone)]
pub(crate) struct FlightValue {
    pub artifact: FlightArtifact,
    pub generated: bool,
}

#[derive(Clone)]
pub(crate) struct FlightArtifact {
    pub manifest: ArtifactManifest,
    pub media_type: NonEmptyText,
    pub encoded_byte_len: u64,
}

impl From<StoredArtifact> for FlightArtifact {
    fn from(artifact: StoredArtifact) -> Self {
        Self {
            manifest: artifact.manifest,
            media_type: artifact.media_type,
            encoded_byte_len: artifact.encoded_bytes.len() as u64,
        }
    }
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
