use std::{
    future::pending,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

use krometrail_core::{CancellationSignal, ErrorCode, KrometrailError, NonEmptyText, Result};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::epoch::{AdaptationLimits, WorkCancellation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactWorkLimits {
    pub max_active_requests: NonZeroUsize,
    pub max_blocking_jobs: NonZeroUsize,
    pub max_parallel_generators_per_request: NonZeroUsize,
    pub max_source_frames: NonZeroUsize,
    pub max_encoded_source_bytes: NonZeroUsize,
    pub max_dimension: NonZeroU32,
    pub max_pixels_per_frame: NonZeroUsize,
    pub max_decoded_bytes: NonZeroUsize,
    pub max_normalized_bytes: NonZeroUsize,
    pub max_combined_request_bytes: NonZeroUsize,
    pub max_outputs: NonZeroUsize,
    pub max_output_bytes_each: NonZeroUsize,
    pub max_output_bytes_total: NonZeroUsize,
    pub max_markers: NonZeroUsize,
    pub max_wall_time: Duration,
}

impl ArtifactWorkLimits {
    pub(crate) fn validate(self) -> Result<Self> {
        if self.max_wall_time.is_zero() {
            return Err(limit_error("artifact wall-time limit must be non-zero"));
        }
        if self.max_decoded_bytes.get() > self.max_combined_request_bytes.get()
            || self.max_normalized_bytes.get() > self.max_combined_request_bytes.get()
            || self.max_output_bytes_total.get() > self.max_combined_request_bytes.get()
            || self.max_output_bytes_each.get() > self.max_output_bytes_total.get()
        {
            return Err(limit_error(
                "artifact memory and output limits are internally inconsistent",
            ));
        }
        if self.max_combined_request_bytes.get() > u32::MAX as usize {
            return Err(limit_error(
                "artifact combined memory limit exceeds scheduler capacity",
            ));
        }
        Ok(self)
    }

    pub(crate) fn adaptation(self) -> AdaptationLimits {
        AdaptationLimits {
            max_source_frames: self.max_source_frames.get(),
            max_encoded_source_bytes: self.max_encoded_source_bytes.get(),
            max_dimension: self.max_dimension.get(),
            max_pixels_per_frame: self.max_pixels_per_frame.get(),
            max_decoded_bytes: self.max_decoded_bytes.get(),
            max_markers: self.max_markers.get(),
        }
    }
}

impl Default for ArtifactWorkLimits {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let cpu_jobs = cpus.saturating_sub(1).clamp(1, 4);
        Self {
            max_active_requests: NonZeroUsize::new(2).unwrap(),
            max_blocking_jobs: NonZeroUsize::new(cpu_jobs).unwrap(),
            max_parallel_generators_per_request: NonZeroUsize::new(2).unwrap(),
            max_source_frames: NonZeroUsize::new(240).unwrap(),
            max_encoded_source_bytes: NonZeroUsize::new(512 * 1024 * 1024).unwrap(),
            max_dimension: NonZeroU32::new(8192).unwrap(),
            max_pixels_per_frame: NonZeroUsize::new(16_777_216).unwrap(),
            max_decoded_bytes: NonZeroUsize::new(1536 * 1024 * 1024).unwrap(),
            max_normalized_bytes: NonZeroUsize::new(512 * 1024 * 1024).unwrap(),
            max_combined_request_bytes: NonZeroUsize::new(2 * 1024 * 1024 * 1024).unwrap(),
            max_outputs: NonZeroUsize::new(16).unwrap(),
            max_output_bytes_each: NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
            max_output_bytes_total: NonZeroUsize::new(256 * 1024 * 1024).unwrap(),
            max_markers: NonZeroUsize::new(256).unwrap(),
            max_wall_time: Duration::from_secs(15),
        }
    }
}

pub(crate) struct ArtifactScheduler {
    limits: ArtifactWorkLimits,
    requests: Arc<Semaphore>,
    cpu: Arc<Semaphore>,
    memory: Arc<Semaphore>,
}

impl ArtifactScheduler {
    pub(crate) fn new(limits: ArtifactWorkLimits) -> Result<Self> {
        let limits = limits.validate()?;
        Ok(Self {
            limits,
            requests: Arc::new(Semaphore::new(limits.max_active_requests.get())),
            cpu: Arc::new(Semaphore::new(limits.max_blocking_jobs.get())),
            memory: Arc::new(Semaphore::new(limits.max_combined_request_bytes.get())),
        })
    }

    pub(crate) const fn limits(&self) -> ArtifactWorkLimits {
        self.limits
    }

    pub(crate) async fn acquire_request(
        &self,
        deadline: Instant,
        cancellation: Option<&Arc<dyn CancellationSignal>>,
    ) -> Result<OwnedSemaphorePermit> {
        controlled(
            self.requests.clone().acquire_owned(),
            deadline,
            cancellation,
        )
        .await?
        .map_err(|_| scheduler_error("artifact request scheduler is closed"))
    }

    pub(crate) async fn acquire_memory(
        &self,
        bytes: usize,
        deadline: Instant,
        cancellation: &WorkCancellation,
    ) -> Result<OwnedSemaphorePermit> {
        if bytes == 0 || bytes > self.limits.max_combined_request_bytes.get() {
            return Err(limit_error(
                "artifact request exceeds the combined memory limit",
            ));
        }
        let permits = u32::try_from(bytes)
            .map_err(|_| limit_error("artifact memory reservation exceeds scheduler capacity"))?;
        controlled_work(
            self.memory.clone().acquire_many_owned(permits),
            deadline,
            cancellation,
        )
        .await?
        .map_err(|_| scheduler_error("artifact memory scheduler is closed"))
    }

    pub(crate) async fn acquire_generator(
        &self,
        semaphore: Arc<Semaphore>,
        deadline: Instant,
        cancellation: &WorkCancellation,
    ) -> Result<OwnedSemaphorePermit> {
        controlled_work(semaphore.acquire_owned(), deadline, cancellation)
            .await?
            .map_err(|_| scheduler_error("artifact generator scheduler is closed"))
    }

    pub(crate) async fn run_blocking<T, F>(
        &self,
        deadline: Instant,
        cancellation: &WorkCancellation,
        operation: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T> + Send + 'static,
    {
        let cpu = controlled_work(self.cpu.clone().acquire_owned(), deadline, cancellation)
            .await?
            .map_err(|_| scheduler_error("artifact CPU scheduler is closed"))?;
        cancellation.check()?;
        let result = tokio::task::spawn_blocking(move || {
            let _cpu = cpu;
            operation()
        });
        controlled_work(result, deadline, cancellation)
            .await?
            .map_err(|_| scheduler_error("artifact blocking worker stopped"))?
    }

    pub(crate) fn generator_semaphore(&self) -> Arc<Semaphore> {
        Arc::new(Semaphore::new(
            self.limits.max_parallel_generators_per_request.get(),
        ))
    }
}

pub(crate) async fn controlled<T>(
    future: impl std::future::Future<Output = T>,
    deadline: Instant,
    cancellation: Option<&Arc<dyn CancellationSignal>>,
) -> Result<T> {
    tokio::select! {
        value = future => Ok(value),
        () = external_cancelled(cancellation) => Err(cancelled_error()),
        () = sleep_until(deadline) => Err(deadline_error()),
    }
}

async fn controlled_work<T>(
    future: impl std::future::Future<Output = T>,
    deadline: Instant,
    cancellation: &WorkCancellation,
) -> Result<T> {
    tokio::select! {
        value = future => Ok(value),
        () = cancellation.cancelled() => Err(cancelled_error()),
        () = sleep_until(deadline) => Err(deadline_error()),
    }
}

async fn external_cancelled(cancellation: Option<&Arc<dyn CancellationSignal>>) {
    match cancellation {
        Some(signal) => signal.cancelled().await,
        None => pending().await,
    }
}

async fn sleep_until(deadline: Instant) {
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
}

pub(crate) fn deadline_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new("artifact generation deadline elapsed")
            .expect("static deadline error is non-empty"),
    )
}

pub(crate) fn cancelled_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("artifact generation was cancelled")
            .expect("static cancellation error is non-empty"),
    )
}

fn scheduler_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message).unwrap(),
    )
}

pub(crate) fn limit_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).unwrap(),
    )
    .with_recovery(
        NonEmptyText::new("narrow the source range or reduce the artifact scope")
            .expect("scheduler limit recovery is non-empty"),
    )
}

pub(crate) fn resource_limit_error(
    subject: impl Into<String>,
    actual: impl std::fmt::Display,
    limit: impl std::fmt::Display,
    recovery: impl Into<String>,
) -> KrometrailError {
    KrometrailError::limit_exceeded(
        ErrorCode::ResourceLimitExceeded,
        subject,
        actual,
        limit,
        None::<String>,
    )
    .with_recovery(NonEmptyText::new(recovery.into()).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_limits() -> ArtifactWorkLimits {
        ArtifactWorkLimits {
            max_active_requests: NonZeroUsize::new(1).unwrap(),
            max_blocking_jobs: NonZeroUsize::new(1).unwrap(),
            max_parallel_generators_per_request: NonZeroUsize::new(1).unwrap(),
            max_source_frames: NonZeroUsize::new(1).unwrap(),
            max_encoded_source_bytes: NonZeroUsize::new(8).unwrap(),
            max_dimension: NonZeroU32::new(1).unwrap(),
            max_pixels_per_frame: NonZeroUsize::new(1).unwrap(),
            max_decoded_bytes: NonZeroUsize::new(8).unwrap(),
            max_normalized_bytes: NonZeroUsize::new(8).unwrap(),
            max_combined_request_bytes: NonZeroUsize::new(8).unwrap(),
            max_outputs: NonZeroUsize::new(1).unwrap(),
            max_output_bytes_each: NonZeroUsize::new(8).unwrap(),
            max_output_bytes_total: NonZeroUsize::new(8).unwrap(),
            max_markers: NonZeroUsize::new(1).unwrap(),
            max_wall_time: Duration::from_secs(1),
        }
    }

    #[tokio::test]
    async fn request_cpu_memory_and_generator_permits_are_independent() {
        let scheduler = ArtifactScheduler::new(tiny_limits()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let request = scheduler.acquire_request(deadline, None).await.unwrap();
        let cancelled = WorkCancellation::default();
        cancelled.cancel();
        let signal: Arc<dyn CancellationSignal> = Arc::new(cancelled.clone());
        assert_eq!(
            scheduler
                .acquire_request(deadline, Some(&signal))
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
        drop(request);

        let memory = scheduler
            .acquire_memory(8, deadline, &WorkCancellation::default())
            .await
            .unwrap();
        assert_eq!(
            scheduler
                .acquire_memory(1, deadline, &cancelled)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
        drop(memory);

        let cpu = scheduler.cpu.clone().acquire_owned().await.unwrap();
        assert_eq!(
            scheduler
                .run_blocking(deadline, &cancelled, || Ok(()))
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
        drop(cpu);

        let generators = scheduler.generator_semaphore();
        let generator = scheduler
            .acquire_generator(
                Arc::clone(&generators),
                deadline,
                &WorkCancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            scheduler
                .acquire_generator(generators, deadline, &cancelled)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
        drop(generator);
    }
}
