//! Bounded, transport-neutral CDP screencast ingestion.
#![allow(dead_code)]
//!
//! Only [`CaptureConfig`] is part of the adapter's composition surface. The coordinator and its
//! per-target resources stay private until supervised-session wiring gives them one lifecycle
//! owner.

use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

use krometrail_core::{
    CaptureFailure, CaptureFailureStage, CaptureStatistics, CaptureStreamState,
    CaptureTimingSummary, DeviceScaleFactor, ErrorCode, ErrorContext, EveryNthFrame, ImageFormat,
    KrometrailError, PixelDimensions, RetryAdvice, SessionId, SessionOrigin, TargetCaptureStatus,
    TargetId,
};

use crate::transport::{CdpTransport, TransportError, TransportSessionId};

pub(crate) mod image_header;
mod pipeline;

#[cfg(test)]
mod tests;

const HARD_MAX_ACTIVE_STREAMS: usize = 32;
const HARD_MAX_QUEUE_CAPACITY: usize = 16;
const HARD_MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAX_QUEUED_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureConfig {
    pub format: ImageFormat,
    pub jpeg_quality: Option<u8>,
    pub max_dimensions: Option<PixelDimensions>,
    pub max_active_streams: NonZeroUsize,
    pub queue_capacity: NonZeroUsize,
    pub max_base64_payload_bytes: NonZeroUsize,
    pub gap_ledger_capacity: NonZeroUsize,
    pub ack_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            format: ImageFormat::Jpeg,
            jpeg_quality: Some(80),
            max_dimensions: None,
            max_active_streams: NonZeroUsize::new(8).expect("default stream count is non-zero"),
            queue_capacity: NonZeroUsize::new(4).expect("default queue capacity is non-zero"),
            max_base64_payload_bytes: NonZeroUsize::new(8 * 1024 * 1024)
                .expect("default payload size is non-zero"),
            gap_ledger_capacity: NonZeroUsize::new(64)
                .expect("default gap ledger size is non-zero"),
            ack_timeout: Duration::from_secs(1),
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

impl CaptureConfig {
    pub(crate) fn validate(&self) -> Result<(), CaptureError> {
        if self.max_active_streams.get() > HARD_MAX_ACTIVE_STREAMS {
            return Err(CaptureError::InvalidConfig("active stream cap is 32"));
        }
        if self.queue_capacity.get() > HARD_MAX_QUEUE_CAPACITY {
            return Err(CaptureError::InvalidConfig("queue capacity cap is 16"));
        }
        if self.max_base64_payload_bytes.get() > HARD_MAX_PAYLOAD_BYTES {
            return Err(CaptureError::InvalidConfig("payload cap is 16 MiB"));
        }
        if self.gap_ledger_capacity.get() == 0 {
            return Err(CaptureError::InvalidConfig(
                "gap ledger capacity is non-zero",
            ));
        }
        if self.ack_timeout.is_zero() || self.shutdown_timeout.is_zero() {
            return Err(CaptureError::InvalidConfig("capture timeouts are non-zero"));
        }
        match self.format {
            ImageFormat::Jpeg if !matches!(self.jpeg_quality, Some(1..=100)) => {
                return Err(CaptureError::InvalidConfig(
                    "JPEG quality must be between 1 and 100",
                ));
            }
            ImageFormat::Png if self.jpeg_quality.is_some() => {
                return Err(CaptureError::InvalidConfig(
                    "PNG capture cannot specify JPEG quality",
                ));
            }
            _ => {}
        }
        let queued_payload_bytes = self
            .max_active_streams
            .get()
            .checked_mul(self.queue_capacity.get())
            .and_then(|slots| slots.checked_mul(self.max_base64_payload_bytes.get()))
            .ok_or(CaptureError::InvalidConfig(
                "queued payload size arithmetic overflow",
            ))?;
        if queued_payload_bytes > MAX_QUEUED_PAYLOAD_BYTES {
            return Err(CaptureError::InvalidConfig(
                "queued payload budget exceeds 256 MiB",
            ));
        }
        Ok(())
    }

    pub(crate) const fn max_queued_payload_bytes() -> usize {
        MAX_QUEUED_PAYLOAD_BYTES
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CaptureError {
    #[error("invalid capture configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("capture transport operation failed")]
    Transport(#[from] TransportError),
    #[error("invalid screencast frame: {0}")]
    InvalidFrame(&'static str),
    #[error("capture task ended")]
    TaskClosed,
}

#[derive(Clone)]
pub(crate) struct CaptureDependencies {
    pub(crate) clock: Arc<dyn krometrail_core::MonotonicClock>,
    pub(crate) ids: Arc<dyn krometrail_core::IdSource>,
    pub(crate) sink: Arc<dyn krometrail_core::RecordingSink>,
    pub(crate) retention: Arc<dyn krometrail_core::RetentionStore>,
}

#[derive(Clone, Debug)]
pub(crate) struct CaptureTarget {
    pub(crate) session_id: SessionId,
    pub(crate) session_origin: SessionOrigin,
    pub(crate) target_id: TargetId,
    pub(crate) connection_generation: u64,
    pub(crate) attachment_generation: u64,
    pub(crate) transport_session: TransportSessionId,
    pub(crate) geometry: CaptureGeometry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CaptureGeometry {
    pub(crate) viewport: PixelDimensions,
    pub(crate) device_scale_factor: DeviceScaleFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CaptureGeometryTransition {
    target_id: TargetId,
    attachment_generation: u64,
    revision: u64,
}

impl CaptureGeometryTransition {
    pub(crate) const fn target_id(self) -> TargetId {
        self.target_id
    }

    pub(crate) const fn attachment_generation(self) -> u64 {
        self.attachment_generation
    }
}

pub(crate) trait CaptureObserver: Send + Sync {
    fn status_changed(&self, status: krometrail_core::TargetCaptureStatus);
    fn startup_failed(&self, _context: crate::targets::CaptureEffectContext) {}

    fn gap_declared(&self, gap: krometrail_core::CaptureGap);

    fn frame_event_stream_closed(&self, _connection_generation: u64) {}

    fn capture_stream_failed(&self, _connection_generation: u64) {}

    fn visibility_changed(
        &self,
        _target_id: TargetId,
        _visibility: krometrail_core::TargetVisibility,
        _observed_at: krometrail_core::SessionTime,
    ) {
    }

    fn geometry_refresh_requested(&self, _transition: CaptureGeometryTransition) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StreamKey {
    target_id: TargetId,
    attachment_generation: u64,
}

pub(crate) struct CaptureCoordinator {
    config: CaptureConfig,
    every_nth_frame: EveryNthFrame,
    dependencies: CaptureDependencies,
    observer: Arc<dyn CaptureObserver>,
    streams: Mutex<std::collections::HashMap<StreamKey, Arc<pipeline::StreamRuntime>>>,
    /// Keys admitted against the cap but not yet registered in `streams`. A start does subscription
    /// and task setup across several awaits before it can insert a runtime, so without a
    /// reservation every concurrent start reads the same pre-insertion `streams` and the cap is
    /// overshot. Only ever locked while `streams` is already held, which fixes the lock order and
    /// makes admission a single atomic step.
    pending_starts: Mutex<std::collections::HashSet<StreamKey>>,
    ordinals: Arc<pipeline::OrdinalRegistry>,
    startup_tasks: Mutex<std::collections::HashMap<StreamKey, tokio::task::JoinHandle<()>>>,
    startup_failures: Mutex<std::collections::HashMap<TargetId, TargetCaptureStatus>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptureStopReason {
    TargetClosed,
    TargetDetached,
    TargetFailed,
    SessionStopping,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaptureStopOutcome {
    pub(crate) complete: bool,
    pub(crate) abandoned_accepted_frames: u64,
    pub(crate) capture_failure: Option<krometrail_core::CaptureFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaptureShutdownOutcome {
    pub(crate) flush_attempted: bool,
    pub(crate) flush_succeeded: bool,
    pub(crate) complete: bool,
    pub(crate) capture_failure: Option<krometrail_core::CaptureFailure>,
}

impl CaptureCoordinator {
    pub(crate) fn new(
        config: CaptureConfig,
        every_nth_frame: EveryNthFrame,
        dependencies: CaptureDependencies,
        observer: Arc<dyn CaptureObserver>,
    ) -> Result<Self, CaptureError> {
        config.validate()?;
        Ok(Self {
            config,
            every_nth_frame,
            dependencies,
            observer,
            streams: Mutex::new(std::collections::HashMap::new()),
            pending_starts: Mutex::new(std::collections::HashSet::new()),
            ordinals: Arc::new(pipeline::OrdinalRegistry::default()),
            startup_tasks: Mutex::new(std::collections::HashMap::new()),
            startup_failures: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Optional startup is owned by capture, independently of any waiting control operation.
    pub(crate) fn spawn_startup(
        &self,
        context: &crate::targets::CaptureEffectContext,
        future: impl std::future::Future<Output = ()> + Send + 'static,
    ) {
        let key = StreamKey {
            target_id: context.target_id,
            attachment_generation: context.attachment_generation,
        };
        self.startup_failures
            .lock()
            .expect("startup failure lock")
            .remove(&context.target_id);
        let mut tasks = self.startup_tasks.lock().expect("startup task lock");
        tasks.retain(|_, task| !task.is_finished());
        if let Some(previous) = tasks.insert(key, tokio::spawn(future)) {
            previous.abort();
        }
    }

    pub(crate) fn startup_failed(
        &self,
        context: crate::targets::CaptureEffectContext,
        stage: CaptureFailureStage,
        message: krometrail_core::NonEmptyText,
    ) {
        let cause = KrometrailError::new(ErrorCode::CaptureFailed, message)
            .with_context(ErrorContext {
                target_id: Some(context.target_id),
                ..ErrorContext::default()
            })
            .with_retry(RetryAdvice::AfterRecovery);
        tracing::warn!(
            event = "capture.startup.failed",
            target_id = %context.target_id,
            failure_stage = stage.as_str(),
            error = cause.message.as_str(),
            "capture.startup.failed"
        );
        let status = TargetCaptureStatus::new(
            context.target_id,
            context.attachment_generation,
            CaptureStreamState::Failed,
            CaptureStatistics::default(),
            self.config.queue_capacity.get(),
            0,
            None,
            CaptureTimingSummary::empty(),
            CaptureTimingSummary::empty(),
            self.every_nth_frame,
            Some(CaptureFailure::new(stage, cause).expect("startup capture failure is valid")),
        )
        .expect("startup capture status is valid");
        self.startup_failures
            .lock()
            .expect("startup failure lock")
            .insert(context.target_id, status.clone());
        self.observer.status_changed(status);
        self.observer.startup_failed(context);
    }

    pub(crate) fn retire_startup_failure(&self, target_id: TargetId, generation: Option<u64>) {
        self.startup_failures
            .lock()
            .expect("startup failure lock")
            .retain(|id, status| {
                *id != target_id
                    || generation
                        .is_some_and(|generation| status.attachment_generation() != generation)
            });
    }

    pub(crate) fn startup_pending(&self, target_id: TargetId, attachment_generation: u64) -> bool {
        self.startup_tasks
            .lock()
            .expect("startup task lock")
            .get(&StreamKey {
                target_id,
                attachment_generation,
            })
            .is_some_and(|task| !task.is_finished())
    }

    pub(crate) fn startup_has_failed(
        &self,
        context: &crate::targets::CaptureEffectContext,
    ) -> bool {
        self.startup_failures
            .lock()
            .expect("startup failure lock")
            .get(&context.target_id)
            .is_some_and(|status| status.attachment_generation() == context.attachment_generation)
    }

    pub(crate) async fn cancel_startup(&self, target_id: TargetId, attachment_generation: u64) {
        let task = self
            .startup_tasks
            .lock()
            .expect("startup task lock")
            .remove(&StreamKey {
                target_id,
                attachment_generation,
            });
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }

    pub(crate) async fn start_target(
        &self,
        target: CaptureTarget,
        transport: Arc<dyn CdpTransport>,
    ) -> Result<(), CaptureError> {
        let result = pipeline::start_target(self, target.clone(), transport).await;
        if result.is_ok() {
            self.streams
                .lock()
                .expect("capture registry lock poisoned")
                .retain(|key, runtime| {
                    key.target_id != target.target_id
                        || key.attachment_generation >= target.attachment_generation
                        || matches!(
                            runtime.state(),
                            krometrail_core::CaptureStreamState::Capturing
                                | krometrail_core::CaptureStreamState::PausedBudget
                        )
                });
        }
        result
    }

    pub(crate) async fn stop_target(
        &self,
        target: &CaptureTarget,
        reason: CaptureStopReason,
        deadline: tokio::time::Instant,
    ) -> CaptureStopOutcome {
        self.cancel_startup(target.target_id, target.attachment_generation)
            .await;
        self.retire_startup_failure(target.target_id, Some(target.attachment_generation));
        pipeline::stop_target(self, target, reason, deadline).await
    }

    pub(crate) async fn suspend_target(
        &self,
        target: &CaptureTarget,
        at: krometrail_core::SessionTime,
    ) {
        self.cancel_startup(target.target_id, target.attachment_generation)
            .await;
        pipeline::suspend_target(self, target, at).await;
    }

    pub(crate) fn every_nth_frame(&self) -> EveryNthFrame {
        self.every_nth_frame
    }

    pub(crate) fn statuses(&self) -> Vec<krometrail_core::TargetCaptureStatus> {
        pipeline::statuses(self)
    }

    pub(crate) fn begin_geometry_transition(
        &self,
        target_id: TargetId,
        attachment_generation: u64,
    ) -> Option<CaptureGeometryTransition> {
        pipeline::begin_geometry_transition(self, target_id, attachment_generation)
            .map(|(transition, _started)| transition)
    }

    pub(crate) fn commit_geometry_transition(
        &self,
        transition: CaptureGeometryTransition,
        geometry: CaptureGeometry,
    ) -> bool {
        pipeline::commit_geometry_transition(self, transition, geometry)
    }

    #[cfg(test)]
    pub(crate) fn geometry_for_test(
        &self,
        target_id: TargetId,
        attachment_generation: u64,
    ) -> Option<(CaptureGeometry, bool)> {
        pipeline::geometry_for_test(self, target_id, attachment_generation)
    }

    pub(crate) async fn shutdown(
        &self,
        session_id: SessionId,
        deadline: tokio::time::Instant,
    ) -> CaptureShutdownOutcome {
        let tasks = std::mem::take(&mut *self.startup_tasks.lock().expect("startup task lock"));
        for task in tasks.into_values() {
            task.abort();
            let _ = task.await;
        }
        let outcome = pipeline::shutdown(self, session_id, deadline).await;
        self.startup_failures
            .lock()
            .expect("startup failure lock")
            .clear();
        outcome
    }
}
