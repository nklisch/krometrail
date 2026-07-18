use std::{sync::Arc, time::Instant};

use krometrail_core::{
    ErrorCode, KrometrailError, NonEmptyText, PortFuture, RetryAdvice, TemporalVideoEncoder,
    VideoEncodeRequest, VideoEncodedClip, VideoEncoderIdentity, VideoEncodingContext,
};

use crate::{
    control::OperationControl,
    discovery::QualifiedExecutable,
    error::{AdapterFailure, AdapterFailureKind},
    qualification::encode_validated,
};

pub struct QualifiedFfmpegEncoder {
    pub(crate) executable: QualifiedExecutable,
    pub(crate) identity: VideoEncoderIdentity,
    pub(crate) permit: Arc<tokio::sync::Semaphore>,
}

impl TemporalVideoEncoder for QualifiedFfmpegEncoder {
    fn identity(&self) -> &VideoEncoderIdentity {
        &self.identity
    }

    fn encode(
        &self,
        request: VideoEncodeRequest,
        context: VideoEncodingContext,
    ) -> PortFuture<'_, krometrail_core::Result<VideoEncodedClip>> {
        Box::pin(async move {
            check_context(&context)?;
            let control =
                OperationControl::new(Arc::clone(&context.cancellation), context.deadline);
            let permit = tokio::select! {
                biased;
                _ = context.cancellation.cancelled() => return Err(core_error(
                    ErrorCode::Cancelled,
                    "temporal video encoding was cancelled before admission",
                )),
                _ = tokio::time::sleep_until(context.deadline.into()) => return Err(core_error(
                    ErrorCode::VideoEncodingFailed,
                    "temporal video encoding exceeded its deadline while waiting for admission",
                )),
                permit = self.permit.acquire() => permit.map_err(|_| core_error(
                    ErrorCode::VideoEncodingFailed,
                    "temporal video encoder admission is unavailable",
                ))?,
            };
            check_context(&context)?;
            self.executable
                .validate_unchanged(&control)
                .await
                .map_err(map_adapter_failure)?;

            let profile = request.profile();
            let validated = encode_validated(&self.executable, &request, &control)
                .await
                .map_err(map_adapter_failure)?;
            drop(permit);
            let identity = self.identity.clone();
            control
                .run_blocking(
                    crate::error::AdapterFailureStage::OutputValidation,
                    move |_| {
                        VideoEncodedClip::new(
                            identity,
                            profile,
                            validated.output_hash,
                            validated.bytes,
                        )
                        .map_err(|failure| {
                            AdapterFailure::new(
                                crate::error::AdapterFailureStage::OutputValidation,
                                if failure.code == ErrorCode::ResourceLimitExceeded {
                                    AdapterFailureKind::OutputOverflow
                                } else {
                                    AdapterFailureKind::Internal
                                },
                            )
                        })
                    },
                )
                .await
                .map_err(map_adapter_failure)
        })
    }
}

fn check_context(context: &VideoEncodingContext) -> krometrail_core::Result<()> {
    if context.cancellation.is_cancelled() {
        return Err(core_error(
            ErrorCode::Cancelled,
            "temporal video encoding was cancelled",
        ));
    }
    if Instant::now() >= context.deadline {
        return Err(core_error(
            ErrorCode::VideoEncodingFailed,
            "temporal video encoding deadline elapsed",
        ));
    }
    Ok(())
}

fn map_adapter_failure(failure: AdapterFailure) -> KrometrailError {
    failure.trace();
    match failure.kind {
        AdapterFailureKind::Cancelled => core_error(
            ErrorCode::Cancelled,
            "temporal video encoding was cancelled",
        ),
        AdapterFailureKind::OutputOverflow => core_error(
            ErrorCode::ResourceLimitExceeded,
            "temporal video encoder exceeded the request output byte limit",
        ),
        AdapterFailureKind::InvalidCandidate | AdapterFailureKind::ChangedCandidate => core_error(
            ErrorCode::VideoEncoderUnavailable,
            "the qualified temporal video encoder is no longer available",
        ),
        AdapterFailureKind::Deadline
        | AdapterFailureKind::UnrepresentableTiming
        | AdapterFailureKind::Spawn
        | AdapterFailureKind::ProcessExit
        | AdapterFailureKind::ProcessIo
        | AdapterFailureKind::ProcessCleanup
        | AdapterFailureKind::StdoutOverflow
        | AdapterFailureKind::DiagnosticOverflow
        | AdapterFailureKind::InvalidOutput
        | AdapterFailureKind::Internal => core_error(
            ErrorCode::VideoEncodingFailed,
            "the qualified temporal video encoder failed to produce a valid bounded clip",
        ),
    }
}

fn core_error(code: ErrorCode, message: &'static str) -> KrometrailError {
    let mut error = KrometrailError::new(
        code,
        NonEmptyText::new(message).expect("static FFmpeg adapter messages are non-empty"),
    )
    .with_retry(code.default_retry());
    if let Some(recovery) = code.default_recovery() {
        error = error.with_recovery(
            NonEmptyText::new(recovery).expect("static core recovery messages are non-empty"),
        );
    } else if code == ErrorCode::ResourceLimitExceeded {
        error = error.with_retry(RetryAdvice::Never);
    }
    error
}

impl QualifiedFfmpegEncoder {
    pub(crate) fn new(executable: QualifiedExecutable, identity: VideoEncoderIdentity) -> Self {
        Self {
            executable,
            identity,
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub const fn identity(&self) -> &VideoEncoderIdentity {
        &self.identity
    }
}
