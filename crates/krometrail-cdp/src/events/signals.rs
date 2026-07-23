use krometrail_core::ObservedTime;
use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSignalKind {
    Lifecycle,
    DialogOpening,
    /// `Page.windowOpen`: an open attempt was made; no blocked/succeeded claim.
    WindowOpen,
    /// `Page.frameRequestedNavigation` with disposition `download`.
    DownloadRequested,
    /// A committed main-frame `Page.frameNavigated` or main-frame
    /// `Page.navigatedWithinDocument`.
    NavigationCommitted,
}

/// One delivered page signal, stamped from the transport ingress receipt when
/// the adapter supplies it, so pump backlog cannot move an earlier event into
/// a later interaction's dispatch..observation interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageSignal {
    pub(crate) kind: PageSignalKind,
    pub(crate) observed_at: ObservedTime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSignalReceiveError {
    Lagged,
    Closed,
}

pub(crate) struct PageSignalReceiver {
    kind: PageSignalKind,
    receiver: broadcast::Receiver<PageSignal>,
}

impl PageSignalReceiver {
    pub(super) fn new(kind: PageSignalKind, receiver: broadcast::Receiver<PageSignal>) -> Self {
        Self { kind, receiver }
    }

    /// Drains already-delivered signals without waiting and counts this
    /// receiver's kind observed inside `floor..=ceiling`. Passive
    /// postcondition observation: lag and closure degrade to a lower-bound
    /// count instead of erroring, because absence of proof is the honest
    /// degraded fact. The time fence keeps a late signal from a previous
    /// interaction (delivered between subscription and dispatch) from being
    /// attributed to the current one.
    pub(crate) fn observed_count_between(
        &mut self,
        floor: ObservedTime,
        ceiling: ObservedTime,
    ) -> u32 {
        let mut count = 0_u32;
        loop {
            match self.receiver.try_recv() {
                Ok(signal)
                    if signal.kind == self.kind
                        && signal.observed_at >= floor
                        && signal.observed_at <= ceiling =>
                {
                    count = count.saturating_add(1);
                }
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => return count,
            }
        }
    }

    /// Fenced boolean form of [`Self::observed_count_between`].
    pub(crate) fn signal_observed_between(
        &mut self,
        floor: ObservedTime,
        ceiling: ObservedTime,
    ) -> bool {
        self.observed_count_between(floor, ceiling) > 0
    }

    pub(crate) async fn recv(&mut self) -> Result<(), PageSignalReceiveError> {
        loop {
            match self.receiver.recv().await {
                Ok(signal) if signal.kind == self.kind => return Ok(()),
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    return Err(PageSignalReceiveError::Lagged);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(PageSignalReceiveError::Closed);
                }
            }
        }
    }

    /// Awaits the next signal of this receiver's kind stamped at or after
    /// `floor`. Pre-floor signals (late deliveries from earlier activity) are
    /// skipped. Lag and closure surface as errors so callers disarm.
    pub(crate) async fn recv_after(
        &mut self,
        floor: ObservedTime,
    ) -> Result<(), PageSignalReceiveError> {
        loop {
            match self.receiver.recv().await {
                Ok(signal) if signal.kind == self.kind && signal.observed_at >= floor => {
                    return Ok(());
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    return Err(PageSignalReceiveError::Lagged);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(PageSignalReceiveError::Closed);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(kind: PageSignalKind, observed_at: u64) -> PageSignal {
        PageSignal {
            kind,
            observed_at: ObservedTime::from_nanos(observed_at),
        }
    }

    fn window(floor: u64, ceiling: u64) -> (ObservedTime, ObservedTime) {
        (
            ObservedTime::from_nanos(floor),
            ObservedTime::from_nanos(ceiling),
        )
    }

    #[test]
    fn passive_drain_counts_only_matching_signals_and_degrades_on_closure() {
        let (sender, receiver) = broadcast::channel(8);
        let mut lifecycle = PageSignalReceiver::new(PageSignalKind::Lifecycle, receiver);
        let (floor, ceiling) = window(0, 100);
        assert_eq!(lifecycle.observed_count_between(floor, ceiling), 0);

        sender
            .send(signal(PageSignalKind::DialogOpening, 10))
            .unwrap();
        assert!(!lifecycle.signal_observed_between(floor, ceiling));

        sender
            .send(signal(PageSignalKind::DialogOpening, 11))
            .unwrap();
        sender.send(signal(PageSignalKind::Lifecycle, 12)).unwrap();
        sender.send(signal(PageSignalKind::Lifecycle, 13)).unwrap();
        assert_eq!(lifecycle.observed_count_between(floor, ceiling), 2);
        // The drain consumed everything delivered so far.
        assert_eq!(lifecycle.observed_count_between(floor, ceiling), 0);

        drop(sender);
        assert!(!lifecycle.signal_observed_between(floor, ceiling));
    }

    /// Consecutive-interaction leakage fence: a signal delivered before the
    /// dispatch floor (queued from the previous interaction) never counts,
    /// and one stamped after the observation ceiling never counts.
    #[test]
    fn drain_fences_attribution_to_the_dispatch_observation_interval() {
        let (sender, receiver) = broadcast::channel(8);
        let mut committed = PageSignalReceiver::new(PageSignalKind::NavigationCommitted, receiver);
        sender
            .send(signal(PageSignalKind::NavigationCommitted, 5))
            .unwrap();
        sender
            .send(signal(PageSignalKind::NavigationCommitted, 15))
            .unwrap();
        sender
            .send(signal(PageSignalKind::NavigationCommitted, 25))
            .unwrap();
        let (floor, ceiling) = window(10, 20);
        assert_eq!(committed.observed_count_between(floor, ceiling), 1);
    }

    #[tokio::test]
    async fn recv_after_skips_pre_floor_and_other_kind_signals() {
        let (sender, receiver) = broadcast::channel(8);
        let mut window_open = PageSignalReceiver::new(PageSignalKind::WindowOpen, receiver);
        sender.send(signal(PageSignalKind::WindowOpen, 5)).unwrap();
        sender.send(signal(PageSignalKind::Lifecycle, 15)).unwrap();
        sender.send(signal(PageSignalKind::WindowOpen, 25)).unwrap();

        assert_eq!(
            window_open.recv_after(ObservedTime::from_nanos(10)).await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn recv_after_reports_closed_receiver() {
        let (sender, receiver) = broadcast::channel(8);
        let mut window_open = PageSignalReceiver::new(PageSignalKind::WindowOpen, receiver);
        drop(sender);

        assert_eq!(
            window_open.recv_after(ObservedTime::from_nanos(0)).await,
            Err(PageSignalReceiveError::Closed)
        );
    }
}
