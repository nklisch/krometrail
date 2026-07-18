use std::{sync::Arc, time::Instant};

use krometrail_core::CancellationSignal;

use crate::error::{AdapterFailure, AdapterFailureKind, AdapterFailureStage};

#[derive(Clone)]
pub(crate) struct OperationControl {
    cancellation: Arc<dyn CancellationSignal>,
    deadline: Instant,
}

impl OperationControl {
    pub(crate) const fn new(cancellation: Arc<dyn CancellationSignal>, deadline: Instant) -> Self {
        Self {
            cancellation,
            deadline,
        }
    }

    pub(crate) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(crate) fn cancellation(&self) -> &dyn CancellationSignal {
        self.cancellation.as_ref()
    }

    pub(crate) fn check(&self, stage: AdapterFailureStage) -> Result<(), AdapterFailure> {
        if self.cancellation.is_cancelled() {
            return Err(AdapterFailure::new(stage, AdapterFailureKind::Cancelled));
        }
        if Instant::now() >= self.deadline {
            return Err(AdapterFailure::new(stage, AdapterFailureKind::Deadline));
        }
        Ok(())
    }

    pub(crate) async fn run_blocking<T, F>(
        &self,
        stage: AdapterFailureStage,
        operation: F,
    ) -> Result<T, AdapterFailure>
    where
        T: Send + 'static,
        F: FnOnce(OperationControl) -> Result<T, AdapterFailure> + Send + 'static,
    {
        self.check(stage)?;
        let worker_control = self.clone();
        let worker = tokio::task::spawn_blocking(move || operation(worker_control));
        tokio::pin!(worker);
        let result = tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => {
                return Err(AdapterFailure::new(stage, AdapterFailureKind::Cancelled));
            }
            _ = tokio::time::sleep_until(self.deadline.into()) => {
                return Err(AdapterFailure::new(stage, AdapterFailureKind::Deadline));
            }
            result = &mut worker => result.map_err(|_| {
                AdapterFailure::new(stage, AdapterFailureKind::Internal)
            })?,
        }?;
        self.check(stage)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::PortFuture;
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    struct ManualCancellation(AtomicBool);

    impl CancellationSignal for ManualCancellation {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }

        fn cancelled(&self) -> PortFuture<'_, ()> {
            Box::pin(async move {
                while !self.is_cancelled() {
                    tokio::task::yield_now().await;
                }
            })
        }
    }

    #[tokio::test]
    async fn blocking_work_returns_at_cancellation_and_exact_deadline() {
        let cancellation = Arc::new(ManualCancellation(AtomicBool::new(false)));
        let control = OperationControl::new(
            cancellation.clone(),
            Instant::now() + Duration::from_secs(5),
        );
        let trigger = cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.0.store(true, Ordering::SeqCst);
        });
        let failure = control
            .run_blocking(AdapterFailureStage::ExecutableIdentity, |_| {
                std::thread::sleep(Duration::from_secs(1));
                Ok(())
            })
            .await
            .unwrap_err();
        assert_eq!(failure.kind, AdapterFailureKind::Cancelled);

        let control = OperationControl::new(
            Arc::new(ManualCancellation(AtomicBool::new(false))),
            Instant::now() + Duration::from_millis(20),
        );
        let failure = control
            .run_blocking(AdapterFailureStage::OutputValidation, |_| {
                std::thread::sleep(Duration::from_secs(1));
                Ok(())
            })
            .await
            .unwrap_err();
        assert_eq!(failure.kind, AdapterFailureKind::Deadline);
    }
}
