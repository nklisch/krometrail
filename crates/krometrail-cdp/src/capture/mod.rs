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
    EveryNthFrame, ImageFormat, PixelDimensions, SessionId, SessionOrigin, TargetId,
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
            ack_timeout: Duration::from_millis(250),
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
    pub(crate) device_scale_factor: krometrail_core::DeviceScaleFactor,
}

pub(crate) trait CaptureObserver: Send + Sync {
    fn status_changed(&self, status: krometrail_core::TargetCaptureStatus);
    fn gap_declared(&self, gap: krometrail_core::CaptureGap);

    fn visibility_changed(
        &self,
        _target_id: TargetId,
        _visibility: krometrail_core::TargetVisibility,
    ) {
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
    ordinals: Arc<pipeline::OrdinalRegistry>,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaptureShutdownOutcome {
    pub(crate) flush_attempted: bool,
    pub(crate) flush_succeeded: bool,
    pub(crate) complete: bool,
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
            ordinals: Arc::new(pipeline::OrdinalRegistry::default()),
        })
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
        pipeline::stop_target(self, target, reason, deadline).await
    }

    pub(crate) async fn suspend_target(
        &self,
        target: &CaptureTarget,
        at: krometrail_core::SessionTime,
    ) {
        pipeline::suspend_target(self, target, at).await;
    }

    pub(crate) fn every_nth_frame(&self) -> EveryNthFrame {
        self.every_nth_frame
    }

    pub(crate) fn statuses(&self) -> Vec<krometrail_core::TargetCaptureStatus> {
        pipeline::statuses(self)
    }

    pub(crate) fn update_device_scale_factor(
        &self,
        target_id: TargetId,
        attachment_generation: u64,
        device_scale_factor: krometrail_core::DeviceScaleFactor,
    ) -> bool {
        pipeline::update_device_scale_factor(
            self,
            target_id,
            attachment_generation,
            device_scale_factor,
        )
    }

    pub(crate) async fn shutdown(
        &self,
        session_id: SessionId,
        deadline: tokio::time::Instant,
    ) -> CaptureShutdownOutcome {
        pipeline::shutdown(self, session_id, deadline).await
    }
}
