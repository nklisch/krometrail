//! Runtime-neutral subscriber fan-out and reconnect policy values.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub const DEFAULT_RECONNECT_TARGET_LIMIT: usize = 64;
pub const DEFAULT_RECONNECT_ATTACH_CONCURRENCY: usize = 4;

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
    /// Maximum number of recordable targets a reconnect attempt may rebuild.
    pub reconnect_target_limit: usize,
    /// Maximum number of target attachment/domain-restore transactions in flight.
    pub reconnect_attach_concurrency: usize,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            reconnect: ReconnectPolicy::default(),
            subscriber_capacity: 64,
            reconnect_target_limit: DEFAULT_RECONNECT_TARGET_LIMIT,
            reconnect_attach_concurrency: DEFAULT_RECONNECT_ATTACH_CONCURRENCY,
        }
    }
}

impl SupervisorConfig {
    pub fn with_reconnect_bounds(mut self, target_limit: usize, attach_concurrency: usize) -> Self {
        self.reconnect_target_limit = target_limit;
        self.reconnect_attach_concurrency = attach_concurrency;
        self
    }

    pub(crate) fn normalized_reconnect_bounds(&self) -> (usize, usize) {
        (
            self.reconnect_target_limit,
            self.reconnect_attach_concurrency.max(1),
        )
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
    // The registry is the sole owner of senders. Receivers retain only state, so a dropped or
    // terminal registry can release every channel without a receiver keeping a sender alive.
    sender: mpsc::Sender<RevisionedEvent>,
    state: Arc<SubscriberState>,
}

#[derive(Default)]
struct SubscriberState {
    lag: Mutex<Option<SubscriberLag>>,
    last_delivered_revision: Mutex<u64>,
    // A terminal event is kept out-of-band so it cannot be lost when a slow subscriber's bounded
    // queue is full. It is delivered only after the queued non-terminal events have drained.
    terminal: Mutex<Option<RevisionedEvent>>,
}

#[derive(Clone, Debug)]
struct RevisionedEvent {
    revision: u64,
    event: BrowserSessionEvent,
}

struct SubscriberRegistryState {
    subscribers: Vec<Subscriber>,
    terminal: bool,
    next_revision: u64,
}

pub(crate) struct SubscriberRegistry {
    state: Mutex<SubscriberRegistryState>,
    capacity: usize,
    lagged: std::sync::atomic::AtomicU64,
}

impl SubscriberRegistry {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(SubscriberRegistryState {
                subscribers: Vec::new(),
                terminal: false,
                next_revision: 0,
            }),
            capacity: capacity.max(1),
            lagged: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub(crate) fn subscribe(&self) -> Box<dyn BrowserSessionEvents> {
        let (sender, receiver) = mpsc::channel(self.capacity);
        let mut registry = self.state.lock().expect("subscriber registry lock");
        let state = Arc::new(SubscriberState::default());
        *state
            .last_delivered_revision
            .lock()
            .expect("subscriber revision lock") = registry.next_revision;
        if registry.terminal {
            // There is no terminal event for a post-terminal subscriber to receive. Dropping this
            // local sender makes its stream close immediately and consistently.
            drop(sender);
        } else {
            registry.subscribers.push(Subscriber {
                sender,
                state: Arc::clone(&state),
            });
        }
        Box::new(BoundedSessionEvents { receiver, state })
    }

    pub(crate) fn publish(&self, event: BrowserSessionEvent) {
        let mut registry = self.state.lock().expect("subscriber registry lock");
        if registry.terminal {
            return;
        }
        registry.next_revision = registry.next_revision.saturating_add(1);
        let item = RevisionedEvent {
            revision: registry.next_revision,
            event,
        };
        if matches!(
            item.event,
            BrowserSessionEvent::SessionStateChanged {
                state: krometrail_core::BrowserSessionState::Ended
            }
        ) {
            // Store Ended before dropping senders. Receivers first drain their bounded queues,
            // then take this item, which makes terminal ordering deterministic even when Full.
            for subscriber in &registry.subscribers {
                *subscriber
                    .state
                    .terminal
                    .lock()
                    .expect("subscriber terminal lock") = Some(item.clone());
            }
            registry.terminal = true;
            registry.subscribers.clear();
            return;
        }

        let mut closed = Vec::new();
        for (index, subscriber) in registry.subscribers.iter().enumerate() {
            match subscriber.sender.try_send(item.clone()) {
                Ok(()) => {
                    *subscriber
                        .state
                        .last_delivered_revision
                        .lock()
                        .expect("subscriber revision lock") = registry.next_revision;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    let lagged_count = self
                        .lagged
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        .saturating_add(1);
                    tracing::debug!(
                        lagged_count,
                        current_revision = registry.next_revision,
                        "browser.session.subscriber_lagged"
                    );
                    let mut lag = subscriber.state.lag.lock().expect("subscriber lag lock");
                    let from = subscriber
                        .state
                        .last_delivered_revision
                        .lock()
                        .expect("subscriber revision lock")
                        .saturating_add(1);
                    match &mut *lag {
                        Some(existing) => {
                            existing.missed_to_revision = registry.next_revision;
                            existing.current_revision = registry.next_revision;
                        }
                        None => {
                            *lag = Some(SubscriberLag {
                                missed_from_revision: from,
                                missed_to_revision: registry.next_revision,
                                current_revision: registry.next_revision,
                            });
                        }
                    }
                }
                Err(mpsc::error::TrySendError::Closed(_)) => closed.push(index),
            }
        }
        for index in closed.into_iter().rev() {
            registry.subscribers.remove(index);
        }
    }
}

struct BoundedSessionEvents {
    receiver: mpsc::Receiver<RevisionedEvent>,
    state: Arc<SubscriberState>,
}

impl BrowserSessionEvents for BoundedSessionEvents {
    fn next(&mut self) -> PortFuture<'_, Result<Option<BrowserSessionEvent>>> {
        Box::pin(async move {
            if let Some(lag) = self.state.lag.lock().expect("subscriber lag lock").take() {
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
                        .state
                        .last_delivered_revision
                        .lock()
                        .expect("subscriber revision lock") = item.revision;
                    Ok(Some(item.event))
                }
                None => match self
                    .state
                    .terminal
                    .lock()
                    .expect("subscriber terminal lock")
                    .take()
                {
                    Some(item) => {
                        *self
                            .state
                            .last_delivered_revision
                            .lock()
                            .expect("subscriber revision lock") = item.revision;
                        Ok(Some(item.event))
                    }
                    None => Ok(None),
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(state: krometrail_core::BrowserSessionState) -> BrowserSessionEvent {
        BrowserSessionEvent::SessionStateChanged { state }
    }

    #[tokio::test]
    async fn bounded_subscribers_report_revision_ranges_before_terminal_closure() {
        let registry = SubscriberRegistry::new(1);
        let mut events = registry.subscribe();
        registry.publish(state(krometrail_core::BrowserSessionState::Connecting));
        registry.publish(state(krometrail_core::BrowserSessionState::Ready));
        registry.publish(state(krometrail_core::BrowserSessionState::Ended));
        let error = events.next().await.unwrap_err();
        assert!(error.message.as_str().contains("missed revisions 2..=2"));
        assert!(error.message.as_str().contains("current revision 2"));

        let first = events.next().await.unwrap().unwrap();
        assert_eq!(
            first,
            state(krometrail_core::BrowserSessionState::Connecting)
        );
        let ended = events.next().await.unwrap().unwrap();
        assert_eq!(ended, state(krometrail_core::BrowserSessionState::Ended));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), events.next())
                .await
                .expect("terminal stream must close")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn terminal_event_is_delivered_once_and_post_terminal_subscriptions_close() {
        let registry = SubscriberRegistry::new(4);
        let mut events = registry.subscribe();
        registry.publish(state(krometrail_core::BrowserSessionState::Connecting));
        registry.publish(state(krometrail_core::BrowserSessionState::Ended));
        registry.publish(state(krometrail_core::BrowserSessionState::Ended));
        assert!(
            registry
                .state
                .lock()
                .expect("subscriber registry lock")
                .subscribers
                .is_empty()
        );

        assert_eq!(
            events.next().await.unwrap().unwrap(),
            state(krometrail_core::BrowserSessionState::Connecting)
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            state(krometrail_core::BrowserSessionState::Ended)
        );
        assert!(events.next().await.unwrap().is_none());

        let mut late = registry.subscribe();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), late.next())
                .await
                .expect("post-terminal subscription must close")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn full_slow_subscriber_still_receives_ended_without_blocking() {
        let registry = SubscriberRegistry::new(1);
        let mut events = registry.subscribe();
        registry.publish(state(krometrail_core::BrowserSessionState::Connecting));
        registry.publish(state(krometrail_core::BrowserSessionState::Ready));
        registry.publish(state(krometrail_core::BrowserSessionState::Ended));

        let error = events.next().await.unwrap_err();
        assert!(error.message.as_str().contains("missed revisions 2..=2"));
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            state(krometrail_core::BrowserSessionState::Connecting)
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            state(krometrail_core::BrowserSessionState::Ended)
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), events.next())
                .await
                .expect("full subscriber must close after terminal")
                .unwrap()
                .is_none()
        );
    }
}
