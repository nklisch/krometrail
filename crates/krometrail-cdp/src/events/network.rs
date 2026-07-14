use tokio::sync::broadcast;

use super::normalize::NormalizedEvent;

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct NetworkRequestKey(String);

impl NetworkRequestKey {
    pub(crate) fn new(raw: &str) -> Option<Self> {
        (!raw.is_empty() && raw.len() <= 512).then(|| Self(raw.to_owned()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkActivityKind {
    Started,
    Response,
    Finished,
    Failed,
}

#[derive(Clone)]
pub(crate) struct NetworkActivity {
    key: NetworkRequestKey,
    kind: NetworkActivityKind,
    long_lived: bool,
    pub(super) normalized: Vec<NormalizedEvent>,
}

impl NetworkActivity {
    pub(super) fn new(
        key: NetworkRequestKey,
        kind: NetworkActivityKind,
        long_lived: bool,
        normalized: Vec<NormalizedEvent>,
    ) -> Self {
        Self {
            key,
            kind,
            long_lived,
            normalized,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_event(
        key: NetworkRequestKey,
        kind: NetworkActivityKind,
        long_lived: bool,
    ) -> Self {
        Self::new(key, kind, long_lived, Vec::new())
    }

    pub(crate) const fn kind(&self) -> NetworkActivityKind {
        self.kind
    }

    pub(crate) fn key(&self) -> &NetworkRequestKey {
        &self.key
    }

    pub(crate) const fn long_lived(&self) -> bool {
        self.long_lived
    }
}

pub(crate) struct NetworkActivityReceiver {
    receiver: broadcast::Receiver<NetworkActivity>,
}

impl NetworkActivityReceiver {
    pub(super) fn new(receiver: broadcast::Receiver<NetworkActivity>) -> Self {
        Self { receiver }
    }

    pub(crate) async fn recv(&mut self) -> Result<NetworkActivity, NetworkReceiveError> {
        self.receiver.recv().await.map_err(|error| match error {
            broadcast::error::RecvError::Lagged(_) => NetworkReceiveError::Lagged,
            broadcast::error::RecvError::Closed => NetworkReceiveError::Closed,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkReceiveError {
    Lagged,
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_receiver_reports_lag_instead_of_skipping_silently() {
        let (sender, receiver) = broadcast::channel(1);
        let mut receiver = NetworkActivityReceiver::new(receiver);
        assert!(
            sender
                .send(NetworkActivity::test_event(
                    NetworkRequestKey::new("first").unwrap(),
                    NetworkActivityKind::Started,
                    false,
                ))
                .is_ok()
        );
        assert!(
            sender
                .send(NetworkActivity::test_event(
                    NetworkRequestKey::new("second").unwrap(),
                    NetworkActivityKind::Finished,
                    false,
                ))
                .is_ok()
        );
        assert!(matches!(
            receiver.recv().await,
            Err(NetworkReceiveError::Lagged)
        ));
    }
}
