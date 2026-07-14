//! Bundle-service error helpers and the shared deadline/cancellation wrapper.
//!
//! Every port await in the bundle service is wrapped by `controlled` so that the
//! bundle's absolute deadline and cancellation signal are honored uniformly. The
//! artifact service additionally receives the same deadline/cancellation through
//! `ArtifactGenerationContext`; the wrapper here is the outer guard that catches
//! a port call that does not respect the context on its own.

use std::{sync::Arc, time::Instant};

use krometrail_core::{
    CancellationSignal, ErrorCode, ErrorContext, KrometrailError, NonEmptyText, ResolvedRange,
    RetryAdvice,
};

/// Wraps a port future with the bundle's absolute deadline and cancellation.
///
/// Returns the inner value on success, `cancelled_error()` if the cancellation
/// signal fires, or `deadline_error()` if the deadline elapses.
pub(crate) async fn controlled<T>(
    future: impl std::future::Future<Output = T>,
    deadline: Instant,
    cancellation: Option<&Arc<dyn CancellationSignal>>,
) -> Result<T, KrometrailError> {
    tokio::select! {
        value = future => Ok(value),
        () = external_cancelled(cancellation) => Err(cancelled_error()),
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
            Err(deadline_error())
        }
    }
}

async fn external_cancelled(cancellation: Option<&Arc<dyn CancellationSignal>>) {
    match cancellation {
        Some(signal) => signal.cancelled().await,
        None => std::future::pending().await,
    }
}

/// The bundle request was cancelled before or during orchestration.
pub(crate) fn cancelled_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("temporal debug bundle request was cancelled")
            .expect("static cancellation error is non-empty"),
    )
    .with_retry(RetryAdvice::Safe)
}

/// The bundle wall-time deadline elapsed before orchestration completed.
pub(crate) fn deadline_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("temporal debug bundle deadline elapsed")
            .expect("static deadline error is non-empty"),
    )
    .with_retry(RetryAdvice::Safe)
}

/// Source or session evidence was evicted or deleted after successful range
/// resolution. The partial bundle is discarded and the caller is advised to
/// re-resolve.
pub(crate) fn evidence_lifetime_error(range: &ResolvedRange) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new(
            "resolved source or session evidence is no longer retained; re-resolve the range",
        )
        .expect("static lifetime error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(range.session_id),
        target_id: Some(range.target_id),
        range: Some(range.resolved_range),
        ..ErrorContext::default()
    })
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("resolve the temporal range again before requesting a bundle")
            .expect("static lifetime recovery is non-empty"),
    )
}

/// No useful evidence remains: both artifact outcomes and context are
/// unavailable. The bundle cannot present any evidence.
pub(crate) fn no_useful_evidence_error(range: &ResolvedRange) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new("temporal debug bundle has no available artifact or context evidence")
            .expect("static no-evidence error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(range.session_id),
        target_id: Some(range.target_id),
        range: Some(range.resolved_range),
        ..ErrorContext::default()
    })
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new(
            "re-resolve the range and retry; inspect recording storage if this repeats",
        )
        .expect("static no-evidence recovery is non-empty"),
    )
}

/// A permit could not be acquired from the bundle semaphore.
pub(crate) fn permit_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new("temporal debug bundle concurrency permit was closed")
            .expect("static permit error is non-empty"),
    )
    .with_retry(RetryAdvice::Safe)
}
