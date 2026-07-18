use std::sync::Arc;

use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserOperationContext,
    BrowserOperationRequest, BrowserOperationResult, BrowserSessionPort, BrowserStatus,
    BrowserStopOutcome, CurrentReferenceGeometry, CurrentReferenceGeometryRequest, ErrorCode,
    KrometrailError, LaunchBrowser, NonEmptyText, PortFuture, ResolvedReferenceGeometry, Result,
    RetryAdvice, TargetCaptureStatus,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct BrowserSessionOwner {
    connector: Arc<dyn BrowserConnector>,
    active: Arc<Mutex<Option<Arc<dyn BrowserSessionPort>>>>,
}

#[derive(Debug)]
pub struct ExecutedBrowserOperation {
    pub(crate) result: BrowserOperationResult,
    pub(crate) capture_statuses: Vec<TargetCaptureStatus>,
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

    pub async fn managed_profiles(&self) -> Result<Vec<krometrail_core::ManagedProfileSummary>> {
        self.connector.managed_profiles().await
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
    ) -> Result<ExecutedBrowserOperation> {
        let session = self.active_session().await?;
        let result = BrowserSessionPort::execute(session.as_ref(), request, context).await?;
        Ok(ExecutedBrowserOperation {
            result,
            capture_statuses: session.capture_statuses(),
        })
    }

    pub async fn capture_statuses(&self) -> Result<Vec<krometrail_core::TargetCaptureStatus>> {
        Ok(self.active_session().await?.capture_statuses())
    }

    pub async fn read_managed_download(
        &self,
        request: krometrail_core::ReadManagedDownloadRequest,
    ) -> Result<krometrail_core::ManagedDownloadRead> {
        let session = self.active.lock().await.clone().ok_or_else(|| {
            KrometrailError::new(
                ErrorCode::NotFound,
                NonEmptyText::new("managed download resource is unavailable").unwrap(),
            )
            .with_recovery(
                NonEmptyText::new("list completed downloads in the active managed browser session")
                    .unwrap(),
            )
        })?;
        session.read_managed_download(request).await
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

impl CurrentReferenceGeometry for BrowserSessionOwner {
    fn current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
        Box::pin(async move {
            let session = self.active_session().await?;
            session.as_ref().current_reference_geometry(request).await
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
        BrowserSessionState, BrowserVersion, CapabilitySupport, CurrentReferenceGeometryRequest,
        ErrorCode, ListPagesRequest, NodeReference, PageStatus, PortFuture, ProfileRef,
        RendererCapability, RetentionStatus, SessionId, SessionOrigin, SnapshotGeneration,
        SnapshotNodeId, TargetId,
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
        fn read_managed_download(
            &self,
            request: krometrail_core::ReadManagedDownloadRequest,
        ) -> PortFuture<'_, Result<krometrail_core::ManagedDownloadRead>> {
            Box::pin(std::future::ready(Ok(
                krometrail_core::ManagedDownloadRead {
                    session_id: request.session_id,
                    download_id: request.download_id,
                    media_type: NonEmptyText::new("application/octet-stream").unwrap(),
                    bytes: b"managed bytes".to_vec(),
                },
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
        fn resolve_current_reference_geometry(
            &self,
            _request: CurrentReferenceGeometryRequest,
        ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
            Box::pin(std::future::ready(Err(KrometrailError::new(
                ErrorCode::StaleReference,
                NonEmptyText::new("fake stale reference").unwrap(),
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
        fn managed_profiles(
            &self,
        ) -> PortFuture<'_, Result<Vec<krometrail_core::ManagedProfileSummary>>> {
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
            krometrail_core::EveryNthFrame::default(),
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

    #[tokio::test]
    async fn managed_download_reads_exist_only_through_the_active_session() {
        let session = Arc::new(FakeSession {
            status: status(),
            execute_calls: AtomicUsize::new(0),
            stop_calls: AtomicUsize::new(0),
        });
        let owner = BrowserSessionOwner::new(Arc::new(FakeConnector {
            session,
            connect_calls: AtomicUsize::new(0),
        }));
        owner
            .attach(AttachBrowser::new("http://127.0.0.1:9222").unwrap())
            .await
            .unwrap();
        let request = krometrail_core::ReadManagedDownloadRequest {
            session_id: "00000000-0000-0000-0000-000000000001".parse().unwrap(),
            download_id: "00000000-0000-0000-0000-000000000002".parse().unwrap(),
            max_bytes: 64,
        };
        assert_eq!(
            owner.read_managed_download(request).await.unwrap().bytes,
            b"managed bytes"
        );
        owner.stop().await.unwrap();
        assert_eq!(
            owner.read_managed_download(request).await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn current_geometry_delegates_through_the_active_owner_lifecycle() {
        let session = Arc::new(FakeSession {
            status: status(),
            execute_calls: AtomicUsize::new(0),
            stop_calls: AtomicUsize::new(0),
        });
        let owner = BrowserSessionOwner::new(Arc::new(FakeConnector {
            session: Arc::clone(&session),
            connect_calls: AtomicUsize::new(0),
        }));
        let request = || {
            CurrentReferenceGeometryRequest::new(
                "00000000-0000-0000-0000-000000000001"
                    .parse::<SessionId>()
                    .unwrap(),
                NodeReference {
                    target_id: "00000000-0000-0000-0000-000000000002"
                        .parse::<TargetId>()
                        .unwrap(),
                    generation: SnapshotGeneration::new(1).unwrap(),
                    node_id: SnapshotNodeId::new(1).unwrap(),
                },
            )
            .unwrap()
        };
        assert_eq!(
            owner
                .current_reference_geometry(request())
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidLifecycleTransition
        );
        owner
            .attach(AttachBrowser::new("http://127.0.0.1:9222").unwrap())
            .await
            .unwrap();
        assert_eq!(
            owner
                .current_reference_geometry(request())
                .await
                .unwrap_err()
                .code,
            ErrorCode::StaleReference
        );
        owner.stop().await.unwrap();
        assert_eq!(
            owner
                .current_reference_geometry(request())
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidLifecycleTransition
        );
    }
}
