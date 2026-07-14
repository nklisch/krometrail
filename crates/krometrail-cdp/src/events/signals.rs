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
