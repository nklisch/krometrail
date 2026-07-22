use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSignalKind {
    Lifecycle,
    DialogOpening,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSignalReceiveError {
    Lagged,
    Closed,
}

pub(crate) struct PageSignalReceiver {
    kind: PageSignalKind,
    receiver: broadcast::Receiver<PageSignalKind>,
}

impl PageSignalReceiver {
    pub(super) fn new(kind: PageSignalKind, receiver: broadcast::Receiver<PageSignalKind>) -> Self {
        Self { kind, receiver }
    }

    /// Drains already-delivered signals without waiting and reports whether
    /// one of this receiver's kind arrived. Passive postcondition
    /// observation: lag and closure degrade to "not observed" instead of
    /// erroring, because absence of proof is the honest degraded fact.
    pub(crate) fn signal_observed(&mut self) -> bool {
        loop {
            match self.receiver.try_recv() {
                Ok(kind) if kind == self.kind => return true,
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => return false,
            }
        }
    }

    pub(crate) async fn recv(&mut self) -> Result<(), PageSignalReceiveError> {
        loop {
            match self.receiver.recv().await {
                Ok(kind) if kind == self.kind => return Ok(()),
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

    #[test]
    fn passive_drain_reports_only_matching_signals_and_degrades_on_closure() {
        let (sender, receiver) = broadcast::channel(4);
        let mut lifecycle = PageSignalReceiver::new(PageSignalKind::Lifecycle, receiver);
        assert!(!lifecycle.signal_observed());

        sender.send(PageSignalKind::DialogOpening).unwrap();
        assert!(!lifecycle.signal_observed());

        sender.send(PageSignalKind::DialogOpening).unwrap();
        sender.send(PageSignalKind::Lifecycle).unwrap();
        assert!(lifecycle.signal_observed());
        // The drain consumed everything delivered so far.
        assert!(!lifecycle.signal_observed());

        drop(sender);
        assert!(!lifecycle.signal_observed());
    }
}
