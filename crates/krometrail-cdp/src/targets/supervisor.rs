//! Runtime-neutral subscriber fan-out and reconnect policy values.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use krometrail_core::{
    BrowserSessionEvent, BrowserSessionEvents, ErrorCode, KrometrailError, NonEmptyText,
    PortFuture, Result,
};
use tokio::sync::mpsc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectPolicy {
    pub delays: Box<[Duration]>,
    pub attempt_timeout: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            delays: vec![
                Duration::from_millis(50),
                Duration::from_millis(100),
                Duration::from_millis(250),
                Duration::from_millis(500),
                Duration::from_secs(1),
            ]
            .into_boxed_slice(),
            attempt_timeout: Duration::from_secs(3),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupervisorConfig {
    pub reconnect: ReconnectPolicy,
    pub subscriber_capacity: usize,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            reconnect: ReconnectPolicy::default(),
            subscriber_capacity: 64,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriberLag {
    pub missed_from_revision: u64,
    pub missed_to_revision: u64,
    pub current_revision: u64,
}

impl SubscriberLag {
    pub fn error(&self) -> KrometrailError {
        KrometrailError::from_browser_failure(
            ErrorCode::BrowserDisconnected,
            NonEmptyText::new(format!(
                "subscriber lagged; refresh targets (missed revisions {}..={}, current revision {})",
                self.missed_from_revision,
                self.missed_to_revision,
                self.current_revision
            ))
            .expect("lag message is non-empty"),
        )
    }
}

struct Subscriber {
    sender: mpsc::Sender<RevisionedEvent>,
    lag: Mutex<Option<SubscriberLag>>,
    last_delivered_revision: Mutex<u64>,
}

#[derive(Clone, Debug)]
struct RevisionedEvent {
    revision: u64,
    event: BrowserSessionEvent,
}

pub(crate) struct SubscriberRegistry {
    subscribers: Mutex<Vec<Arc<Subscriber>>>,
    capacity: usize,
    next_revision: std::sync::atomic::AtomicU64,
    lagged: std::sync::atomic::AtomicU64,
}

impl SubscriberRegistry {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            subscribers: Mutex::new(Vec::new()),
            capacity: capacity.max(1),
            next_revision: std::sync::atomic::AtomicU64::new(0),
            lagged: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub(crate) fn subscribe(&self) -> Box<dyn BrowserSessionEvents> {
        let (sender, receiver) = mpsc::channel(self.capacity);
        let subscriber = Arc::new(Subscriber {
            sender,
            lag: Mutex::new(None),
            last_delivered_revision: Mutex::new(
                self.next_revision
                    .load(std::sync::atomic::Ordering::Acquire),
            ),
        });
        self.subscribers
            .lock()
            .expect("subscriber registry lock")
            .push(Arc::clone(&subscriber));
        Box::new(BoundedSessionEvents {
            receiver,
            subscriber,
        })
    }

    pub(crate) fn publish(&self, event: BrowserSessionEvent) {
        let revision = self
            .next_revision
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1);
        let item = RevisionedEvent { revision, event };
        let mut closed = Vec::new();
        let mut subscribers = self.subscribers.lock().expect("subscriber registry lock");
        for (index, subscriber) in subscribers.iter().enumerate() {
            match subscriber.sender.try_send(item.clone()) {
                Ok(()) => {
                    *subscriber
                        .last_delivered_revision
                        .lock()
                        .expect("subscriber revision lock") = revision;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    let lagged_count = self
                        .lagged
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        .saturating_add(1);
                    tracing::debug!(
                        lagged_count,
                        current_revision = revision,
                        "browser.session.subscriber_lagged"
                    );
                    let mut lag = subscriber.lag.lock().expect("subscriber lag lock");
                    let from = subscriber
                        .last_delivered_revision
                        .lock()
                        .expect("subscriber revision lock")
                        .saturating_add(1);
                    match &mut *lag {
                        Some(existing) => {
                            existing.missed_to_revision = revision;
                            existing.current_revision = revision;
                        }
                        None => {
                            *lag = Some(SubscriberLag {
                                missed_from_revision: from,
                                missed_to_revision: revision,
                                current_revision: revision,
                            });
                        }
                    }
                }
                Err(mpsc::error::TrySendError::Closed(_)) => closed.push(index),
            }
        }
        for index in closed.into_iter().rev() {
            subscribers.remove(index);
        }
    }
}

struct BoundedSessionEvents {
    receiver: mpsc::Receiver<RevisionedEvent>,
    subscriber: Arc<Subscriber>,
}

impl BrowserSessionEvents for BoundedSessionEvents {
    fn next(&mut self) -> PortFuture<'_, Result<Option<BrowserSessionEvent>>> {
        Box::pin(async move {
            if let Some(lag) = self
                .subscriber
                .lag
                .lock()
                .expect("subscriber lag lock")
                .take()
            {
                tracing::info!(
                    missed_from_revision = lag.missed_from_revision,
                    missed_to_revision = lag.missed_to_revision,
                    current_revision = lag.current_revision,
                    "browser.session.subscriber_lagged"
                );
                return Err(lag.error());
            }
            match self.receiver.recv().await {
                Some(item) => {
                    *self
                        .subscriber
                        .last_delivered_revision
                        .lock()
                        .expect("subscriber revision lock") = item.revision;
                    Ok(Some(item.event))
                }
                None => Ok(None),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_subscribers_report_revision_ranges_without_backpressure() {
        let registry = SubscriberRegistry::new(1);
        let mut events = registry.subscribe();
        registry.publish(BrowserSessionEvent::SessionStateChanged {
            state: krometrail_core::BrowserSessionState::Connecting,
        });
        registry.publish(BrowserSessionEvent::SessionStateChanged {
            state: krometrail_core::BrowserSessionState::Ready,
        });
        registry.publish(BrowserSessionEvent::SessionStateChanged {
            state: krometrail_core::BrowserSessionState::Ended,
        });
        let error = events.next().await.unwrap_err();
        assert!(error.message.as_str().contains("missed revisions 2..=3"));
        assert!(error.message.as_str().contains("current revision 3"));
    }
}
