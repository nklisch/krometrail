//! Test-only observable barrier protocol for live browser qualification.
//!
//! The protocol is intentionally a small ordered state machine.  It keeps a transport response,
//! a current observation, and durable capture evidence distinct: a command acknowledgement can
//! never advance the trial past the post-action observation barrier.

use std::{future::Future, time::Duration};

use krometrail_core::{ErrorCode, KrometrailError, NonEmptyText, Result};

pub const DEFAULT_BARRIER_TIMEOUT: Duration = Duration::from_secs(5);

/// The only legal order for one control trial.  The values are also the stable names used by
/// scripted qualification traces; changing the order changes the qualification contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlBarrier {
    BrowserLockAcquired,
    LoopbackServerReady,
    TargetAttached,
    ViewportReported,
    PageReady,
    StructuredOperationSubmitted,
    PostActionObservationPresent,
    FixtureSettled,
    CaptureFenceAcknowledged,
    IntervalQueryComplete,
}

impl ControlBarrier {
    pub const ORDER: [Self; 10] = [
        Self::BrowserLockAcquired,
        Self::LoopbackServerReady,
        Self::TargetAttached,
        Self::ViewportReported,
        Self::PageReady,
        Self::StructuredOperationSubmitted,
        Self::PostActionObservationPresent,
        Self::FixtureSettled,
        Self::CaptureFenceAcknowledged,
        Self::IntervalQueryComplete,
    ];

    pub const fn rank(self) -> usize {
        match self {
            Self::BrowserLockAcquired => 0,
            Self::LoopbackServerReady => 1,
            Self::TargetAttached => 2,
            Self::ViewportReported => 3,
            Self::PageReady => 4,
            Self::StructuredOperationSubmitted => 5,
            Self::PostActionObservationPresent => 6,
            Self::FixtureSettled => 7,
            Self::CaptureFenceAcknowledged => 8,
            Self::IntervalQueryComplete => 9,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BarrierProtocolError {
    OutOfOrder {
        expected: Option<ControlBarrier>,
        received: ControlBarrier,
    },
    TimedOut(ControlBarrier),
    MissingObservation,
}

impl BarrierProtocolError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::OutOfOrder { .. } | Self::MissingObservation => ErrorCode::CaptureRejected,
            Self::TimedOut(_) => ErrorCode::WaitTimedOut,
        }
    }

    pub fn into_error(self) -> KrometrailError {
        let (code, message, recovery) = match self {
            Self::OutOfOrder { .. } => (
                ErrorCode::CaptureRejected,
                "control barrier order was violated",
                "restart the trial from a fresh observable readiness barrier",
            ),
            Self::TimedOut(_) => (
                ErrorCode::WaitTimedOut,
                "control qualification barrier timed out",
                "inspect browser status and establish a fresh readiness barrier before continuing",
            ),
            Self::MissingObservation => (
                ErrorCode::CaptureRejected,
                "control operation did not provide the required live observation",
                "refresh the current page observation before deciding whether to repeat the operation",
            ),
        };
        KrometrailError::new(
            code,
            NonEmptyText::new(message).expect("static barrier error"),
        )
        .with_recovery(NonEmptyText::new(recovery).expect("static barrier recovery"))
        .with_retry(krometrail_core::RetryAdvice::AfterRecovery)
    }
}

/// A trace for one attempt.  A trace may contain only a prefix after a failure, but it can never
/// contain a later barrier without all preceding barriers exactly once.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct BarrierTrace {
    stages: Vec<ControlBarrier>,
}

impl BarrierTrace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(
        &mut self,
        stage: ControlBarrier,
    ) -> std::result::Result<(), BarrierProtocolError> {
        let expected = ControlBarrier::ORDER.get(self.stages.len()).copied();
        if expected != Some(stage) {
            return Err(BarrierProtocolError::OutOfOrder {
                expected,
                received: stage,
            });
        }
        self.stages.push(stage);
        Ok(())
    }

    pub fn stages(&self) -> &[ControlBarrier] {
        &self.stages
    }

    pub fn is_complete(&self) -> bool {
        self.stages.as_slice() == ControlBarrier::ORDER
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        temporal_evaluation::canonical_json(self).map_err(|_| {
            KrometrailError::new(
                ErrorCode::PersistenceFailed,
                NonEmptyText::new("control barrier trace could not be canonicalized")
                    .expect("static barrier serialization error"),
            )
        })
    }
}

/// Await an observable production handle under a safety deadline.  The deadline bounds waiting;
/// it does not create a readiness signal and therefore cannot turn a missing observation into a
/// pass.
pub async fn bounded<T, F>(stage: ControlBarrier, deadline: Duration, future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    tokio::time::timeout(deadline, future)
        .await
        .map_err(|_| BarrierProtocolError::TimedOut(stage).into_error())?
}

pub fn barrier_order_is_valid(stages: &[ControlBarrier]) -> bool {
    stages
        .iter()
        .enumerate()
        .all(|(index, stage)| ControlBarrier::ORDER.get(index) == Some(stage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_barrier_order_is_exact_and_canonical() {
        let mut trace = BarrierTrace::new();
        for stage in ControlBarrier::ORDER {
            trace.record(stage).unwrap();
        }
        assert!(trace.is_complete());
        assert!(barrier_order_is_valid(trace.stages()));
        assert_eq!(
            trace.canonical_bytes().unwrap(),
            trace.canonical_bytes().unwrap()
        );
    }

    #[test]
    fn acknowledgement_cannot_skip_the_post_observation_barrier() {
        let mut trace = BarrierTrace::new();
        for stage in [
            ControlBarrier::BrowserLockAcquired,
            ControlBarrier::LoopbackServerReady,
            ControlBarrier::TargetAttached,
            ControlBarrier::ViewportReported,
            ControlBarrier::PageReady,
            ControlBarrier::StructuredOperationSubmitted,
        ] {
            trace.record(stage).unwrap();
        }
        let error = trace.record(ControlBarrier::FixtureSettled).unwrap_err();
        assert_eq!(
            error,
            BarrierProtocolError::OutOfOrder {
                expected: Some(ControlBarrier::PostActionObservationPresent),
                received: ControlBarrier::FixtureSettled,
            }
        );
        assert!(!trace.is_complete());
    }

    #[tokio::test]
    async fn bounded_barrier_wait_returns_a_timeout_without_sleeping() {
        let result = bounded(
            ControlBarrier::TargetAttached,
            Duration::from_millis(1),
            async { std::future::pending::<Result<()>>().await },
        )
        .await;
        assert_eq!(result.unwrap_err().code, ErrorCode::WaitTimedOut);
    }
}
