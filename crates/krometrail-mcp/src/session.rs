use std::sync::Arc;

use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserOperationContext,
    BrowserOperationRequest, BrowserOperationResult, BrowserSessionPort, BrowserStatus,
    BrowserStopOutcome, ErrorCode, KrometrailError, LaunchBrowser, NonEmptyText, Result,
    RetryAdvice,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct BrowserSessionOwner {
    connector: Arc<dyn BrowserConnector>,
    active: Arc<Mutex<Option<Arc<dyn BrowserSessionPort>>>>,
}

impl BrowserSessionOwner {
    pub fn new(connector: Arc<dyn BrowserConnector>) -> Self {
        Self {
            connector,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(&self, request: LaunchBrowser) -> Result<BrowserStatus> {
        self.connect(BrowserConnectRequest::Launch(request)).await
    }

    pub async fn attach(&self, request: AttachBrowser) -> Result<BrowserStatus> {
        self.connect(BrowserConnectRequest::Attach(request)).await
    }

    async fn connect(&self, request: BrowserConnectRequest) -> Result<BrowserStatus> {
        // Keep the slot locked until the candidate proves it can report status. This prevents two
        // concurrent lifecycle calls from creating competing browser owners.
        let mut active = self.active.lock().await;
        if active.is_some() {
            return Err(lifecycle_error(
                "a browser session is already active",
                "stop the active browser session before starting or attaching another",
            ));
        }
        let session = self.connector.connect(request).await?;
        let status = session.status().await?;
        *active = Some(session);
        Ok(status)
    }

    pub async fn status(&self) -> Result<BrowserStatus> {
        self.active_session().await?.status().await
    }

    pub async fn execute(
        &self,
        request: BrowserOperationRequest,
        context: BrowserOperationContext,
    ) -> Result<BrowserOperationResult> {
        let session = self.active_session().await?;
        BrowserSessionPort::execute(session.as_ref(), request, context).await
    }

    pub async fn stop(&self) -> Result<BrowserStopOutcome> {
        let session = self.active.lock().await.take().ok_or_else(|| {
            lifecycle_error(
                "no browser session is active",
                "start or attach a browser session before stopping it",
            )
        })?;
        session.stop().await
    }

    pub async fn shutdown(&self) -> Result<()> {
        let Some(session) = self.active.lock().await.take() else {
            return Ok(());
        };
        session.stop().await.map(|_| ())
    }

    async fn active_session(&self) -> Result<Arc<dyn BrowserSessionPort>> {
        self.active.lock().await.clone().ok_or_else(|| {
            lifecycle_error(
                "no browser session is active",
                "start or attach a browser session before calling browser tools",
            )
        })
    }
}

fn lifecycle_error(message: &'static str, recovery: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidLifecycleTransition,
        NonEmptyText::new(message).expect("static lifecycle message is valid"),
    )
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(NonEmptyText::new(recovery).expect("static lifecycle recovery is valid"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        BrowserCompatibility, BrowserInstallation, BrowserOperationResult, BrowserOwnership,
        BrowserProduct, BrowserProductVersion, BrowserSessionEvent, BrowserSessionEvents,
        BrowserSessionState, BrowserVersion, CapabilitySupport, ListPagesRequest, PageStatus,
        PortFuture, ProfileRef, RendererCapability, RetentionStatus, SessionId, SessionOrigin,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ClosedEvents;
    impl BrowserSessionEvents for ClosedEvents {
        fn next(&mut self) -> PortFuture<'_, Result<Option<BrowserSessionEvent>>> {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    struct FakeSession {
        status: BrowserStatus,
        execute_calls: AtomicUsize,
        stop_calls: AtomicUsize,
    }

    impl BrowserSessionPort for FakeSession {
        fn session_origin(&self) -> SessionOrigin {
            SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0))
        }
        fn status(&self) -> PortFuture<'_, Result<BrowserStatus>> {
            Box::pin(std::future::ready(Ok(self.status.clone())))
        }
        fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>> {
            Box::pin(std::future::ready(Ok(
                Box::new(ClosedEvents) as Box<dyn BrowserSessionEvents>
            )))
        }
        fn execute(
            &self,
            _request: BrowserOperationRequest,
            _context: BrowserOperationContext,
        ) -> PortFuture<'_, Result<BrowserOperationResult>> {
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(Err(KrometrailError::new(
                ErrorCode::Unsupported,
                NonEmptyText::new("fake operation result").unwrap(),
            ))))
        }
        fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>> {
            self.stop_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(Ok(BrowserStopOutcome::Detached)))
        }
    }

    struct FakeConnector {
        session: Arc<FakeSession>,
        connect_calls: AtomicUsize,
    }

    impl BrowserConnector for FakeConnector {
        fn installations(&self) -> PortFuture<'_, Result<Vec<BrowserInstallation>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }
        fn connect(
            &self,
            _request: BrowserConnectRequest,
        ) -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>> {
            self.connect_calls.fetch_add(1, Ordering::SeqCst);
            let session = Arc::clone(&self.session) as Arc<dyn BrowserSessionPort>;
            Box::pin(std::future::ready(Ok(session)))
        }
    }

    fn status() -> BrowserStatus {
        let version = BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("1").unwrap(),
            "revision",
            "1.3",
            "agent",
            "1",
        )
        .unwrap();
        let compatibility = BrowserCompatibility::new(
            version,
            RendererCapability::ALL
                .iter()
                .map(|capability| CapabilitySupport::new(*capability, true, true, None).unwrap())
                .collect(),
        )
        .unwrap();
        BrowserStatus::new(
            "00000000-0000-0000-0000-000000000001"
                .parse::<SessionId>()
                .unwrap(),
            BrowserSessionState::Ready,
            BrowserOwnership::Attached,
            ProfileRef::External,
            compatibility,
            None,
            Vec::<PageStatus>::new(),
            Vec::new(),
            RetentionStatus::empty(krometrail_core::DiskBudgetBytes::default()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn owner_serializes_lifecycle_dispatches_once_and_removes_before_stop() {
        let session = Arc::new(FakeSession {
            status: status(),
            execute_calls: AtomicUsize::new(0),
            stop_calls: AtomicUsize::new(0),
        });
        let connector = Arc::new(FakeConnector {
            session: Arc::clone(&session),
            connect_calls: AtomicUsize::new(0),
        });
        let owner = BrowserSessionOwner::new(connector.clone());
        assert_eq!(
            owner.status().await.unwrap_err().code,
            ErrorCode::InvalidLifecycleTransition
        );

        owner
            .attach(AttachBrowser::new("http://127.0.0.1:9222").unwrap())
            .await
            .unwrap();
        assert_eq!(connector.connect_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            owner
                .start(LaunchBrowser::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidLifecycleTransition
        );
        assert_eq!(connector.connect_calls.load(Ordering::SeqCst), 1);
        let error = owner
            .execute(
                BrowserOperationRequest::ListPages(ListPagesRequest),
                BrowserOperationContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(session.execute_calls.load(Ordering::SeqCst), 1);

        assert_eq!(owner.stop().await.unwrap(), BrowserStopOutcome::Detached);
        assert_eq!(session.stop_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            owner.status().await.unwrap_err().code,
            ErrorCode::InvalidLifecycleTransition
        );
        owner.shutdown().await.unwrap();
        assert_eq!(session.stop_calls.load(Ordering::SeqCst), 1);
    }
}
